// Package app contains the main Bubbletea application model.
// This is the central hub that manages app state and delegates to child views.
package app

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/launch"
	"github.com/aayushdutt/mctui/internal/loader"
	"github.com/aayushdutt/mctui/internal/mods"
	"github.com/aayushdutt/mctui/internal/ui"
	"github.com/charmbracelet/bubbles/key"
	tea "github.com/charmbracelet/bubbletea"
)

// State represents the current view/screen of the application
type State int

const (
	StateHome State = iota
	StateNewInstance
	StateLaunch
	StateMods
	StateSettings
	StateAuth
)

// Model is the main application model
type Model struct {
	state  State
	width  int
	height int

	// Child models for each view
	home     *ui.HomeModel
	wizard   *ui.WizardModel
	launch   *ui.LaunchModel
	mods     *ui.ModsModel
	auth     *ui.AuthModel
	settings *ui.SettingsModel

	// Core services
	cfg       *config.Config
	instances *core.InstanceManager
	accounts  *core.AccountManager
	mojang    *api.MojangClient
	modrinth  *api.ModrinthClient

	// Launch state
	launchStatusChan chan launch.Status
	launchCtxCancel  context.CancelFunc

	// Key bindings
	keys keyMap

	// Shared state
	ready bool
}

// keyMap defines the keybindings for the app
type keyMap struct {
	ForceQuit key.Binding
	Quit      key.Binding
	Help      key.Binding
	Back      key.Binding
}

func defaultKeyMap() keyMap {
	return keyMap{
		ForceQuit: key.NewBinding(
			key.WithKeys("ctrl+c"),
			key.WithHelp("ctrl+c", "quit"),
		),
		Quit: key.NewBinding(
			key.WithKeys("q"),
			key.WithHelp("q", "quit"),
		),
		Help: key.NewBinding(
			key.WithKeys("?"),
			key.WithHelp("?", "help"),
		),
		Back: key.NewBinding(
			key.WithKeys("esc"),
			key.WithHelp("esc", "back"),
		),
	}
}

// New creates a new application model
func New() *Model {
	cfg, _ := config.Load()
	if !ui.Apply(cfg.Theme) {
		ui.Apply("dark")
		cfg.Theme = "dark"
	}
	cfg.EnsureDirs()

	instances := core.NewInstanceManager(cfg.DataDir)
	accounts := core.NewAccountManager(cfg.DataDir)
	accounts.Load()

	return newWithDeps(
		cfg,
		instances,
		accounts,
		api.NewMojangClient(cfg.DataDir),
		api.NewModrinthClient(),
	)
}

// newWithDeps builds a Model from already-constructed dependencies. It is the
// dependency-injection seam New() delegates to: tests construct temp-dir-backed
// managers and a Modrinth client pointed at a test server, then call this
// directly to exercise the full app without touching the real data dir or
// network. Production callers should use New().
func newWithDeps(cfg *config.Config, instances *core.InstanceManager, accounts *core.AccountManager, mojang *api.MojangClient, modrinth *api.ModrinthClient) *Model {
	home := ui.NewHomeModel()
	home.SetAccountManager(accounts)

	return &Model{
		state:     StateHome,
		home:      home,
		cfg:       cfg,
		instances: instances,
		accounts:  accounts,
		mojang:    mojang,
		modrinth:  modrinth,
		keys:      defaultKeyMap(),
	}
}

// Init implements tea.Model
func (m *Model) Init() tea.Cmd {
	return tea.Batch(
		m.home.Init(),
		m.loadInstances(),
		tea.Sequence(
			func() tea.Msg { return ui.ActiveSessionCheckStarted{} },
			m.checkActiveSessionCmd(),
		),
	)
}

func (m *Model) effectiveMSAClientID() string {
	if m.cfg.MSAClientID != "" {
		return m.cfg.MSAClientID
	}
	return config.DefaultMSAClientID
}

func (m *Model) prepareAuthScreen() tea.Cmd {
	m.state = StateAuth
	m.auth = ui.NewAuthModel(m.cfg.DataDir, m.effectiveMSAClientID(), m.accounts)
	cw, ch := m.contentSize()
	m.auth.SetSize(cw, ch)
	return m.auth.Init()
}

// contentSize is the drawable area inside [ui.AppShellStyle] for the current terminal size.
func (m *Model) contentSize() (w, h int) {
	if m.width <= 0 {
		return 0, 0
	}
	return max(0, m.width-2*ui.AppShellPadX), max(0, m.height-2*ui.AppShellPadY)
}

// validateMSAccessToken reads acc.AccessToken from a command goroutine. This does
// not race with the session-refresh writes in Update: a silent refresh is only
// dispatched for expired accounts (see checkActiveSessionCmd / gateOnlineLaunch,
// both guarded by acc.IsExpired()), whereas this is only ever called for
// non-expired accounts — the two states are mutually exclusive for a given account.
func (m *Model) validateMSAccessToken(ctx context.Context, acc *core.Account) error {
	return api.NewAuthClient(m.effectiveMSAClientID()).ValidateMinecraftToken(ctx, acc.AccessToken)
}

// sessionRefreshedMsg carries the result of a silent MSA refresh attempt back to
// the single-threaded Update loop. The network refresh runs inside a background
// command (see refreshActiveSession); the actual mutation of the *core.Account
// and the AccountManager.Save() happen only when this message is applied in
// Update, mirroring the accountCreatedMsg pattern. This keeps all account/disk
// writes off the concurrent command goroutines and avoids data races.
type sessionRefreshedMsg struct {
	accountID    string
	accessToken  string
	refreshToken string
	expiresAt    time.Time
	err          error // non-nil if the refresh failed
	authError    bool  // true when err is an MSA auth error => must re-login

	// gate routes the post-apply continuation. When launch is non-nil, a
	// successful refresh proceeds to launch; otherwise the home-screen session
	// check result is recomputed.
	launch *core.Instance
}

// refreshActiveSession performs the silent MSA token refresh in a background
// command and reports the outcome via sessionRefreshedMsg. It never mutates the
// account or touches disk — that happens when the message is applied in Update.
func (m *Model) refreshActiveSession(acc *core.Account, inst *core.Instance) tea.Cmd {
	id := acc.ID
	refreshToken := acc.MSARefreshToken
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		defer cancel()
		client := api.NewAuthClient(m.effectiveMSAClientID())
		mcToken, newRefresh, expiresIn, err := client.RefreshSession(ctx, refreshToken)
		if err != nil {
			return sessionRefreshedMsg{
				accountID: id,
				err:       err,
				authError: errors.Is(err, api.ErrMSARefreshInvalid),
				launch:    inst,
			}
		}
		return sessionRefreshedMsg{
			accountID:    id,
			accessToken:  mcToken,
			refreshToken: newRefresh,
			expiresAt:    time.Now().Add(time.Duration(expiresIn) * time.Second),
			launch:       inst,
		}
	}
}

func (m *Model) checkActiveSessionCmd() tea.Cmd {
	return func() tea.Msg {
		acc := m.accounts.GetActive()
		if acc == nil || acc.Type != core.AccountTypeMSA {
			return ui.ActiveSessionCheckResult{Status: ui.ActiveSessionNotApplicable}
		}
		if acc.IsExpired() {
			// Try a silent refresh before forcing a re-login. The actual token
			// swap + Save() is applied in Update when sessionRefreshedMsg lands.
			if acc.MSARefreshToken != "" {
				return refreshTriggerMsg{}
			}
			return ui.ActiveSessionCheckResult{Status: ui.ActiveSessionInvalid}
		}
		ctx, cancel := context.WithTimeout(context.Background(), 12*time.Second)
		defer cancel()
		err := m.validateMSAccessToken(ctx, acc)
		if errors.Is(err, api.ErrMinecraftSessionInvalid) {
			return ui.ActiveSessionCheckResult{Status: ui.ActiveSessionInvalid}
		}
		if err != nil {
			return ui.ActiveSessionCheckResult{Status: ui.ActiveSessionUncertain, Err: err}
		}
		return ui.ActiveSessionCheckResult{Status: ui.ActiveSessionOK}
	}
}

func (m *Model) gateOnlineLaunch(inst *core.Instance) tea.Cmd {
	return func() tea.Msg {
		acc := m.accounts.GetActive()
		if acc == nil {
			return ui.NavigateToAuth{}
		}
		switch acc.Type {
		case core.AccountTypeOffline:
			// No Minecraft Services token; online launch would run with an empty accessToken.
			return ui.NavigateToLaunch{Instance: inst, Offline: true}
		case core.AccountTypeMSA:
			// continue below
		default:
			return ui.NavigateToAuth{}
		}
		if acc.IsExpired() {
			// Attempt a silent refresh so launch proceeds without a detour to the
			// auth screen. Only fall back to re-login if the refresh itself fails
			// with an auth error (handled when sessionRefreshedMsg is applied).
			if acc.MSARefreshToken != "" {
				return launchRefreshTriggerMsg{instance: inst}
			}
			return ui.SessionGateFailed{NeedAuth: true}
		}
		ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		err := m.validateMSAccessToken(ctx, acc)
		if errors.Is(err, api.ErrMinecraftSessionInvalid) {
			return ui.SessionGateFailed{NeedAuth: true}
		}
		if err != nil {
			return ui.SessionGateFailed{Err: err}
		}
		return ui.ProceedWithLaunch{Instance: inst}
	}
}

// refreshTriggerMsg and launchRefreshTriggerMsg are emitted by the session-check
// commands (which run concurrently) to ask the single-threaded Update loop to
// kick off a silent refresh. They carry no account snapshot: Update reads the
// current active account so it always refreshes against the latest stored token.
type refreshTriggerMsg struct{}
type launchRefreshTriggerMsg struct{ instance *core.Instance }

func (m *Model) sessionRecheckCmd() tea.Cmd {
	return tea.Sequence(
		func() tea.Msg { return ui.ActiveSessionCheckStarted{} },
		m.checkActiveSessionCmd(),
	)
}

func (m *Model) loadInstances() tea.Cmd {
	return m.loadInstancesSelecting("")
}

// loadInstancesSelecting reloads instances from disk; selectID optionally focuses that instance in the home list.
func (m *Model) loadInstancesSelecting(selectID string) tea.Cmd {
	return func() tea.Msg {
		err := m.instances.Load()
		return ui.InstancesLoaded{
			Instances: m.instances.List(),
			Error:     err,
			SelectID:  selectID,
		}
	}
}

func (m *Model) loadVersions() tea.Cmd {
	return func() tea.Msg {
		ctx := context.Background()
		manifest, err := m.mojang.GetVersionManifest(ctx)
		if err != nil {
			return ui.VersionsLoaded{Error: err}
		}
		return ui.VersionsLoaded{
			Versions: manifest.Versions,
			Latest:   manifest.Latest.Release,
		}
	}
}

// Update implements tea.Model
func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true

		cw, ch := m.contentSize()
		m.home.SetSize(cw, ch)
		if m.wizard != nil {
			m.wizard.SetSize(cw, ch)
		}
		if m.launch != nil {
			m.launch.SetSize(cw, ch)
		}
		if m.mods != nil {
			m.mods.SetSize(cw, ch)
		}
		if m.auth != nil {
			m.auth.SetSize(cw, ch)
		}
		if m.settings != nil {
			m.settings.SetSize(cw, ch)
		}

	// Navigation messages
	case ui.NavigateToHome:
		if m.mods != nil {
			m.mods.CancelPending()
		}
		m.state = StateHome
		m.mods = nil
		m.settings = nil
		return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())

	case ui.NavigateToSettings:
		m.state = StateSettings
		m.settings = ui.NewSettingsModel(m.cfg)
		cw, ch := m.contentSize()
		m.settings.SetSize(cw, ch)
		return m, m.settings.Init()

	case ui.NavigateToNewInstance:
		m.state = StateNewInstance
		m.wizard = ui.NewWizardModel(m.cfg.ShowSnapshots)
		cw, ch := m.contentSize()
		m.wizard.SetSize(cw, ch)
		return m, tea.Batch(
			m.wizard.Init(),
			m.loadVersions(),
		)

	case ui.NavigateToMods:
		if msg.Instance == nil {
			return m, nil
		}
		m.state = StateMods
		m.mods = ui.NewModsModel(msg.Instance, m.modrinth)
		cw, ch := m.contentSize()
		m.mods.SetSize(cw, ch)
		return m, m.mods.Init()

	case ui.NavigateToLaunch:
		if msg.Offline {
			m.state = StateLaunch
			m.launch = ui.NewLaunchModel(msg.Instance, m.cfg)
			cw, ch := m.contentSize()
			m.launch.SetSize(cw, ch)
			return m, tea.Batch(
				m.launch.Init(),
				m.beginLaunch(msg.Instance, true),
			)
		}
		return m, m.gateOnlineLaunch(msg.Instance)

	case ui.ProceedWithLaunch:
		m.state = StateLaunch
		m.launch = ui.NewLaunchModel(msg.Instance, m.cfg)
		cw, ch := m.contentSize()
		m.launch.SetSize(cw, ch)
		return m, tea.Batch(
			m.launch.Init(),
			m.beginLaunch(msg.Instance, false),
		)

	case ui.SessionGateFailed:
		if msg.NeedAuth {
			return m, m.prepareAuthScreen()
		}
		m.state = StateHome
		if m.launchCtxCancel != nil {
			m.launchCtxCancel()
			m.launchCtxCancel = nil
		}
		m.launch = nil
		m.launchStatusChan = nil
		m.home.SetTransientBanner("Could not verify Microsoft session. Check your connection or press [o] for offline.")
		return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())

	case ui.ActiveSessionCheckStarted:
		m.home.SetSessionCheckStarted()
		return m, nil

	case ui.ActiveSessionCheckResult:
		m.home.ApplyActiveSessionCheckResult(msg)
		return m, nil

	case refreshTriggerMsg:
		// Background check asked us to silently refresh the home-screen session.
		// Read the current active account on the event loop, then dispatch the
		// network refresh as a command.
		acc := m.accounts.GetActive()
		if acc == nil || acc.Type != core.AccountTypeMSA || acc.MSARefreshToken == "" {
			m.home.ApplyActiveSessionCheckResult(ui.ActiveSessionCheckResult{Status: ui.ActiveSessionInvalid})
			return m, nil
		}
		return m, m.refreshActiveSession(acc, nil)

	case launchRefreshTriggerMsg:
		// Online launch gate asked us to silently refresh before launching.
		acc := m.accounts.GetActive()
		if acc == nil || acc.Type != core.AccountTypeMSA || acc.MSARefreshToken == "" {
			return m, func() tea.Msg { return ui.SessionGateFailed{NeedAuth: true} }
		}
		return m, m.refreshActiveSession(acc, msg.instance)

	case sessionRefreshedMsg:
		// Apply the refresh result on the single-threaded event loop: mutate the
		// account and persist via the AccountManager here so all disk writes stay
		// off the concurrent command goroutines. If two refreshes race, Update
		// applies them serially (last write wins; both yield valid tokens).
		if msg.err != nil {
			if msg.authError {
				// Refresh token rejected — must re-login via device code.
				if msg.launch != nil {
					return m, func() tea.Msg { return ui.SessionGateFailed{NeedAuth: true} }
				}
				m.home.ApplyActiveSessionCheckResult(ui.ActiveSessionCheckResult{Status: ui.ActiveSessionInvalid})
				return m, nil
			}
			// Network / transient failure — keep the session, do NOT force re-login.
			if msg.launch != nil {
				return m, func() tea.Msg { return ui.SessionGateFailed{Err: msg.err} }
			}
			m.home.ApplyActiveSessionCheckResult(ui.ActiveSessionCheckResult{Status: ui.ActiveSessionUncertain, Err: msg.err})
			return m, nil
		}
		// Success: persist the new Minecraft token and rotated refresh token.
		if acc := m.accounts.GetActive(); acc != nil && acc.ID == msg.accountID {
			acc.AccessToken = msg.accessToken
			acc.ExpiresAt = msg.expiresAt
			acc.MSARefreshToken = msg.refreshToken
			if err := m.accounts.Save(); err != nil {
				m.home.SetTransientBanner(fmt.Sprintf("Refreshed session, but couldn't save: %v", err))
			}
		}
		if msg.launch != nil {
			return m, func() tea.Msg { return ui.ProceedWithLaunch{Instance: msg.launch} }
		}
		m.home.ApplyActiveSessionCheckResult(ui.ActiveSessionCheckResult{Status: ui.ActiveSessionOK})
		return m, nil

	case ui.NavigateToAuth:
		return m, m.prepareAuthScreen()

	case ui.RetryLoadVersions:
		return m, m.loadVersions()

	case ui.PersistShowSnapshots:
		m.cfg.ShowSnapshots = msg.Value
		if err := m.cfg.Save(); err != nil {
			m.home.SetTransientBanner(fmt.Sprintf("Couldn't save snapshot preference: %v", err))
		}
		return m, nil

	case ui.SettingsSaved:
		m.cfg.JavaPath = msg.JavaPath
		m.cfg.JVMArgs = msg.JVMArgs
		m.cfg.ShowSnapshots = msg.ShowSnapshots
		m.cfg.MSAClientID = msg.MSAClientID
		m.cfg.Theme = msg.Theme
		// The theme was applied live during preview; re-dress the long-lived
		// home list so it picks up the new palette (per-entry screens rebuild
		// themselves on next navigation).
		m.home.ApplyTheme()
		m.state = StateHome
		m.settings = nil
		if err := m.cfg.Save(); err != nil {
			m.home.SetTransientBanner(fmt.Sprintf("Applied for this session, but couldn't write config: %v", err))
		} else {
			m.home.SetTransientBanner("Settings saved.")
		}
		// MSA client ID may have changed; re-validate the active session.
		return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())

	case ui.ModInstallDoneMsg:
		if m.mods != nil {
			newMods, cmd := m.mods.Update(msg)
			m.mods = newMods.(*ui.ModsModel)
			return m, cmd
		}
		return m, nil

	case ui.DeleteInstance:
		if msg.Instance != nil {
			if err := m.instances.Delete(msg.Instance.ID); err != nil {
				m.home.SetTransientBanner(fmt.Sprintf("Couldn't delete instance: %v", err))
			}
			return m, m.loadInstances()
		}
	// Instance management
	case ui.InstanceCreated:
		if err := m.instances.Create(msg.Instance); err != nil {
			m.state = StateHome
			m.home.SetTransientBanner(fmt.Sprintf("Couldn't create instance: %v", err))
			return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())
		}
		m.state = StateHome
		id := msg.Instance.ID
		return m, tea.Batch(m.loadInstancesSelecting(id), m.sessionRecheckCmd())

	// Launch status updates - continue subscription
	case ui.LaunchStatusUpdate:
		var cmd tea.Cmd
		if m.launch != nil {
			_, cmd = m.launch.Update(msg)
		}
		// Continue listening for more status updates
		return m, tea.Batch(cmd, m.waitForLaunchStatus(m.launchStatusChan))

	// Cancel launch - user pressed ESC
	case ui.CancelLaunch:
		// Cancel the context to stop ongoing operations
		if m.launchCtxCancel != nil {
			m.launchCtxCancel()
			m.launchCtxCancel = nil
		}
		// Clean up
		m.launchStatusChan = nil
		// Return to home
		m.state = StateHome
		return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())

	// Launch complete - clean up
	case ui.LaunchComplete:
		var cmd tea.Cmd
		if m.launch != nil {
			newLaunch, c := m.launch.Update(msg)
			m.launch = newLaunch.(*ui.LaunchModel)
			cmd = c
		}
		// Clean up
		m.launchStatusChan = nil
		if m.launchCtxCancel != nil {
			m.launchCtxCancel = nil
		}
		return m, cmd

	// Retry launch
	case ui.RetryLaunch:
		if m.launch != nil {
			inst := m.launch.GetInstance()
			if msg.Offline {
				return m, m.beginLaunch(inst, true)
			}
			return m, m.gateOnlineLaunch(inst)
		}
		return m, nil

	// Global key handlers
	case tea.KeyMsg:
		switch {
		case key.Matches(msg, m.keys.ForceQuit):
			return m, tea.Quit
		case key.Matches(msg, m.keys.Quit):
			if m.state == StateHome {
				return m, tea.Quit
			}
		}
	}

	// Delegate to current view
	switch m.state {
	case StateHome:
		newHome, cmd := m.home.Update(msg)
		m.home = newHome.(*ui.HomeModel)
		cmds = append(cmds, cmd)

	case StateNewInstance:
		if m.wizard != nil {
			newWizard, cmd := m.wizard.Update(msg)
			m.wizard = newWizard.(*ui.WizardModel)
			cmds = append(cmds, cmd)
		}

	case StateAuth:
		if m.auth != nil {
			newAuth, cmd := m.auth.Update(msg)
			m.auth = newAuth.(*ui.AuthModel)
			cmds = append(cmds, cmd)
		}
	case StateLaunch:
		if m.launch != nil {
			newLaunch, cmd := m.launch.Update(msg)
			m.launch = newLaunch.(*ui.LaunchModel)
			cmds = append(cmds, cmd)
		}
	case StateMods:
		if m.mods != nil {
			newMods, cmd := m.mods.Update(msg)
			m.mods = newMods.(*ui.ModsModel)
			cmds = append(cmds, cmd)
		}
	case StateSettings:
		if m.settings != nil {
			newSettings, cmd := m.settings.Update(msg)
			m.settings = newSettings.(*ui.SettingsModel)
			cmds = append(cmds, cmd)
		}
	}

	return m, tea.Batch(cmds...)
}

// beginLaunch sets up the launch context and status channel on the event loop
// (so reads/writes of m.launchCtxCancel and m.launchStatusChan stay single-threaded),
// then hands them to startLaunch's command goroutine.
func (m *Model) beginLaunch(inst *core.Instance, offline bool) tea.Cmd {
	ctx, cancel := context.WithCancel(context.Background())
	m.launchCtxCancel = cancel
	m.launchStatusChan = make(chan launch.Status, 10)

	// Snapshot credentials ON the event loop so the command goroutine never reads
	// the shared *core.Account fields, which Update may concurrently rewrite when a
	// sessionRefreshedMsg lands. Offline launches use the default Player identity.
	playerName := "Player"
	uuid := "00000000-0000-0000-0000-000000000000"
	accessToken := ""
	if !offline {
		if acc := m.accounts.GetActive(); acc != nil {
			playerName = acc.Name
			uuid = acc.ID
			accessToken = acc.AccessToken
		}
	}

	return m.startLaunch(ctx, m.launchStatusChan, inst, offline, playerName, uuid, accessToken)
}

func (m *Model) startLaunch(ctx context.Context, statusChan chan launch.Status, inst *core.Instance, offline bool, playerName, uuid, accessToken string) tea.Cmd {
	return func() tea.Msg {
		// Find version info (vanilla or merged loader profile)
		details, err := loader.ResolveVersionDetails(ctx, m.mojang, inst, offline)
		if err != nil {
			return ui.LaunchComplete{Error: err}
		}
		if loader.ParseKind(inst.Loader) == loader.KindFabric {
			_ = m.instances.Update(inst)
		}

		// Validate version info
		if details.MainClass == "" {
			return ui.LaunchComplete{Error: fmt.Errorf("invalid version info: missing main class")}
		}

		// Player info (playerName/uuid/accessToken) is snapshotted by beginLaunch on
		// the event loop and passed in, so this goroutine never reads the shared
		// *core.Account that a session refresh may rewrite concurrently.

		// Start launcher in goroutine (starter Fabric mods first if requested, with progress on this screen)
		go func() {
			if loader.ParseKind(inst.Loader) == loader.KindFabric && inst.InstallStarterFabricMods {
				if mods.StarterFabricModsComplete(inst) {
					inst.InstallStarterFabricMods = false
					_ = m.instances.Update(inst)
				} else {
					svc := mods.NewService(m.modrinth)
					err := svc.InstallStarterFabricMods(ctx, inst, func(i, total int, label string) {
						var p float64
						if total > 0 {
							p = float64(i) / float64(total) * 0.12
						}
						statusChan <- launch.Status{
							Step:     "Installing starter mods",
							Message:  fmt.Sprintf("Downloading %s (%d/%d). Slow connections may take several minutes.", label, i+1, total),
							Progress: p,
						}
					})
					if err != nil {
						statusChan <- launch.Status{
							Step:    "Installing starter mods",
							Message: err.Error(),
							Error:   err,
						}
						close(statusChan)
						return
					}
					inst.InstallStarterFabricMods = false
					_ = m.instances.Update(inst)
				}
			}

			launcher := launch.NewLauncher(&launch.Options{
				Instance:         inst,
				VersionInfo:      details,
				Offline:          offline,
				PlayerName:       playerName,
				UUID:             uuid,
				AccessToken:      accessToken,
				Config:           m.cfg,
				UpdateLastPlayed: m.instances.UpdateLastPlayed,
				UpdateInstance:   m.instances.Update,
			}, statusChan)

			err := launcher.Launch(ctx)

			// Send final status then close
			if err != nil {
				statusChan <- launch.Status{
					Step:    "Error",
					Message: err.Error(),
					Error:   err,
				}
			}
			close(statusChan)
		}()

		// Return first status update command
		return m.waitForLaunchStatus(statusChan)()
	}
}

// waitForLaunchStatus creates a command that waits for the next launch status.
// The channel is captured by value so the command goroutine never reads the
// m.launchStatusChan field (which the event loop may set to nil on cancel/complete).
func (m *Model) waitForLaunchStatus(ch chan launch.Status) tea.Cmd {
	return func() tea.Msg {
		if ch == nil {
			return ui.LaunchComplete{}
		}

		status, ok := <-ch
		if !ok {
			// Channel closed, launch complete
			return ui.LaunchComplete{}
		}

		if status.Error != nil {
			return ui.LaunchComplete{Error: status.Error}
		}

		if status.IsComplete {
			return ui.LaunchComplete{}
		}

		return ui.LaunchStatusUpdate{Status: status}
	}
}

// View implements tea.Model. All screens go through [ui.AppShellStyle] here only — do not pad in leaf views.
func (m *Model) View() string {
	return ui.AppShellStyle.Background(ui.Active.Background).Render(m.shellContent())
}

// shellContent renders the full-frame body before the app shell. Add a branch for every [State] value
// when introducing new screens so layout width/height and padding stay consistent.
func (m *Model) shellContent() string {
	if !m.ready {
		return "Initializing..."
	}
	switch m.state {
	case StateHome:
		return m.home.View()
	case StateNewInstance:
		if m.wizard != nil {
			return m.wizard.View()
		}
	case StateLaunch:
		if m.launch != nil {
			return m.launch.View()
		}
	case StateAuth:
		if m.auth != nil {
			return m.auth.View()
		}
	case StateMods:
		if m.mods != nil {
			return m.mods.View()
		}
	case StateSettings:
		if m.settings != nil {
			return m.settings.View()
		}
	}
	return "Unknown state"
}
