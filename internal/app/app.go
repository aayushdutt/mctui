// Package app contains the main Bubbletea application model.
// This is the central hub that manages app state and delegates to child views.
package app

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/fsutil"
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
	StateResourcePacks
	StateSettings
	StateEditInstance
	StateAuth
)

// Model is the main application model
type Model struct {
	state  State
	width  int
	height int

	// Child models for each view
	home          *ui.HomeModel
	wizard        *ui.WizardModel
	launch        *ui.LaunchModel
	mods          *ui.ModsModel
	resourcePacks *ui.ResourcePacksModel
	auth          *ui.AuthModel
	settings      *ui.SettingsModel
	editInstance  *ui.EditInstanceModel

	// Core services
	cfg           *config.Config
	instances     *core.InstanceManager
	accounts      *core.AccountManager
	mojang        *api.MojangClient
	modrinth      *api.ModrinthClient
	vanillaTweaks *api.VanillaTweaksClient

	// Launch state. At most one launch runs at a time. launchID increments per
	// launch and tags its messages, so results from a cancelled or replaced
	// launch are recognised and dropped.
	launchID         int
	launchLive       bool // the launch pipeline (and then the game) is running
	launchStopping   bool // the user stopped the running game; waiting for it to exit
	gatingLaunch     bool // an online session check for a launch is in flight
	launchStatusChan chan launch.Status
	launchCtxCancel  context.CancelFunc

	// Key bindings
	keys keyMap

	// Shared state
	ready bool

	// startupWarnings are shown in the home banner on Init (e.g. a config or
	// accounts file that couldn't be read and was moved aside).
	startupWarnings []string

	// quitArmed is set by a first q on home while a game is still running; a
	// second q (with no other key in between) quits.
	quitArmed bool
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
	var warnings []string

	cfg, err := config.Load()
	if err != nil {
		// Never crash on a bad config: start from defaults (a malformed file is
		// moved aside so a later save can't silently destroy it).
		cfg = config.DefaultConfig()
		warnings = append(warnings, recoverUnreadable(filepath.Join(cfg.DataDir, "config.json"), err, "started with default settings", 0))
	}
	if !ui.Apply(cfg.Theme) {
		ui.Apply("dark")
		cfg.Theme = "dark"
	}
	if err := cfg.EnsureDirs(); err != nil {
		warnings = append(warnings, fmt.Sprintf("Couldn't create the data folders: %v", err))
	}

	instances := core.NewInstanceManager(cfg.DataDir)
	accounts := core.NewAccountManager(cfg.DataDir)
	if err := accounts.Load(); err != nil {
		// Load may have partially populated the manager before failing; start
		// clean, and never let a later Save overwrite the file (it holds refresh
		// tokens): a malformed one is moved aside, an unreadable one is left
		// alone and saves are refused this session.
		accounts = core.NewAccountManager(cfg.DataDir)
		if !isMalformedJSON(err) {
			accounts.BlockSaves(err)
		}
		warnings = append(warnings, recoverUnreadable(filepath.Join(cfg.DataDir, "accounts.json"), err, "signed out for now", 0o600))
	}

	m := newWithDeps(
		cfg,
		instances,
		accounts,
		api.NewMojangClient(cfg.DataDir),
		api.NewModrinthClient(),
	)
	m.startupWarnings = warnings
	return m
}

// recoverUnreadable handles a state file that failed to load and returns a
// warning for the home notice; consequence says what the user should expect
// (e.g. "started with default settings"). A malformed file is moved aside (see
// [fsutil.Quarantine]) — chmod'ed to perm when non-zero, since it may hold
// tokens — so a later save can't destroy it. A file that merely couldn't be
// read (permissions, I/O) is left where it is.
func recoverUnreadable(path string, loadErr error, consequence string, perm os.FileMode) string {
	name := filepath.Base(path)
	if !isMalformedJSON(loadErr) {
		return fmt.Sprintf("Couldn't read %s (%v) — %s.", name, loadErr, consequence)
	}
	dest, err := fsutil.Quarantine(path)
	if err != nil {
		return fmt.Sprintf("%s is damaged (%v) — %s.", name, loadErr, consequence)
	}
	if perm != 0 {
		_ = os.Chmod(dest, perm)
	}
	return fmt.Sprintf("%s was damaged — %s. The old file was kept as %s.", name, consequence, filepath.Base(dest))
}

// isMalformedJSON reports whether a load error is about the file's content
// (vs. failing to read it at all).
func isMalformedJSON(err error) bool {
	var syntax *json.SyntaxError
	var typ *json.UnmarshalTypeError
	return errors.As(err, &syntax) || errors.As(err, &typ)
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
		state:         StateHome,
		home:          home,
		cfg:           cfg,
		instances:     instances,
		accounts:      accounts,
		mojang:        mojang,
		modrinth:      modrinth,
		vanillaTweaks: api.NewVanillaTweaksClient(),
		keys:          defaultKeyMap(),
	}
}

// Init implements tea.Model
func (m *Model) Init() tea.Cmd {
	if len(m.startupWarnings) > 0 {
		m.home.SetNotice(m.startupWarnings...)
	}
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

// prepareAuthScreen opens the device-code sign-in. When pendingLaunch is non-nil
// (auth was triggered by an online-launch attempt), the auth screen offers an
// "play offline" escape hatch for that instance.
func (m *Model) prepareAuthScreen(pendingLaunch *core.Instance) tea.Cmd {
	m.state = StateAuth
	m.auth = ui.NewAuthModel(m.cfg.DataDir, m.effectiveMSAClientID(), m.accounts, pendingLaunch)
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
			return ui.SessionGateFailed{NeedAuth: true, Instance: inst}
		}
		switch acc.Type {
		case core.AccountTypeOffline:
			// No Minecraft Services token; online launch would run with an empty accessToken.
			return ui.ProceedWithLaunch{Instance: inst, Offline: true}
		case core.AccountTypeMSA:
			// continue below
		default:
			return ui.SessionGateFailed{NeedAuth: true, Instance: inst}
		}
		if acc.IsExpired() {
			// Attempt a silent refresh so launch proceeds without a detour to the
			// auth screen. Only fall back to re-login if the refresh itself fails
			// with an auth error (handled when sessionRefreshedMsg is applied).
			if acc.MSARefreshToken != "" {
				return launchRefreshTriggerMsg{instance: inst}
			}
			return ui.SessionGateFailed{NeedAuth: true, Instance: inst}
		}
		ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		err := m.validateMSAccessToken(ctx, acc)
		if errors.Is(err, api.ErrMinecraftSessionInvalid) {
			return ui.SessionGateFailed{NeedAuth: true, Instance: inst}
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
		if m.resourcePacks != nil {
			m.resourcePacks.SetSize(cw, ch)
		}
		if m.auth != nil {
			m.auth.SetSize(cw, ch)
		}
		if m.settings != nil {
			m.settings.SetSize(cw, ch)
		}
		if m.editInstance != nil {
			m.editInstance.SetSize(cw, ch)
		}

	// Navigation messages
	case ui.NavigateToHome:
		if m.mods != nil {
			m.mods.CancelPending()
		}
		if m.resourcePacks != nil {
			m.resourcePacks.CancelPending()
		}
		m.state = StateHome
		m.mods = nil
		m.resourcePacks = nil
		m.settings = nil
		m.editInstance = nil
		return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())

	case ui.NavigateToEditInstance:
		if msg.Instance == nil {
			return m, nil
		}
		if cmd := m.blockIfRunning(msg.Instance, "settings"); cmd != nil {
			return m, cmd
		}
		m.state = StateEditInstance
		m.editInstance = ui.NewEditInstanceModel(msg.Instance)
		cw, ch := m.contentSize()
		m.editInstance.SetSize(cw, ch)
		return m, m.editInstance.Init()

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
		if cmd := m.blockIfRunning(msg.Instance, "mods"); cmd != nil {
			return m, cmd
		}
		m.state = StateMods
		m.mods = ui.NewModsModel(msg.Instance, m.modrinth)
		cw, ch := m.contentSize()
		m.mods.SetSize(cw, ch)
		return m, m.mods.Init()

	case ui.NavigateToResourcePacks:
		if msg.Instance == nil {
			return m, nil
		}
		if cmd := m.blockIfRunning(msg.Instance, "resource packs"); cmd != nil {
			return m, cmd
		}
		m.state = StateResourcePacks
		m.resourcePacks = ui.NewResourcePacksModel(msg.Instance, m.vanillaTweaks)
		cw, ch := m.contentSize()
		m.resourcePacks.SetSize(cw, ch)
		return m, m.resourcePacks.Init()

	case ui.NavigateToLaunch:
		// One launch at a time: a second Enter while the session check or a
		// launch is running must not start another (two JVMs on one game dir,
		// and the first launch would become uncancellable).
		if m.gatingLaunch {
			return m, nil
		}
		if m.launchLive {
			// The game kept running after the user left its screen: Enter on
			// that instance reopens it; any other instance waits its turn.
			if running := m.runningInstance(); running != nil && m.state != StateLaunch {
				if msg.Instance != nil && msg.Instance.ID == running.ID {
					m.state = StateLaunch
					cw, ch := m.contentSize()
					m.launch.SetSize(cw, ch)
					return m, nil
				}
				return m, m.home.SetTransientBanner(fmt.Sprintf("%s is still running — close Minecraft before launching another instance.", running.Name))
			}
			return m, nil
		}
		if msg.Offline {
			return m, m.openLaunchScreen(msg.Instance, true)
		}
		m.gatingLaunch = true
		m.home.SetLaunchGate(msg.Instance.Name)
		return m, m.gateOnlineLaunch(msg.Instance)

	case ui.ProceedWithLaunch:
		m.gatingLaunch = false
		m.home.SetLaunchGate("")
		if m.launchLive {
			return m, nil
		}
		return m, m.openLaunchScreen(msg.Instance, msg.Offline)

	case ui.SessionGateFailed:
		m.gatingLaunch = false
		m.home.SetLaunchGate("")
		if msg.NeedAuth {
			return m, m.prepareAuthScreen(msg.Instance)
		}
		m.state = StateHome
		if !m.launchLive {
			m.launch = nil
		}
		bannerCmd := m.home.SetTransientBanner("Could not verify Microsoft session. Check your connection or press [o] for offline.")
		return m, tea.Batch(bannerCmd, m.loadInstances(), m.sessionRecheckCmd())

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
			inst := msg.instance
			return m, func() tea.Msg { return ui.SessionGateFailed{NeedAuth: true, Instance: inst} }
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
					inst := msg.launch
					return m, func() tea.Msg { return ui.SessionGateFailed{NeedAuth: true, Instance: inst} }
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
		var bannerCmd tea.Cmd
		if acc := m.accounts.GetActive(); acc != nil && acc.ID == msg.accountID {
			acc.AccessToken = msg.accessToken
			acc.ExpiresAt = msg.expiresAt
			// refresh_token is optional in an OAuth refresh response: keep the
			// stored one unless Microsoft actually rotated it, or every later
			// expiry would force a full device-code sign-in.
			if msg.refreshToken != "" {
				acc.MSARefreshToken = msg.refreshToken
			}
			if err := m.accounts.Save(); err != nil {
				bannerCmd = m.home.SetTransientBanner(fmt.Sprintf("Refreshed session, but couldn't save: %v", err))
			}
		}
		if msg.launch != nil {
			return m, tea.Batch(bannerCmd, func() tea.Msg { return ui.ProceedWithLaunch{Instance: msg.launch} })
		}
		m.home.ApplyActiveSessionCheckResult(ui.ActiveSessionCheckResult{Status: ui.ActiveSessionOK})
		return m, bannerCmd

	case ui.NavigateToAuth:
		return m, m.prepareAuthScreen(nil)

	case ui.RetryLoadVersions:
		return m, m.loadVersions()

	case ui.PersistShowSnapshots:
		m.cfg.ShowSnapshots = msg.Value
		if err := m.cfg.Save(); err != nil {
			return m, m.home.SetTransientBanner(fmt.Sprintf("Couldn't save snapshot preference: %v", err))
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
		var bannerCmd tea.Cmd
		if err := m.cfg.Save(); err != nil {
			bannerCmd = m.home.SetTransientBanner(fmt.Sprintf("Applied for this session, but couldn't write config: %v", err))
		} else {
			bannerCmd = m.home.SetTransientBanner("Settings saved.")
		}
		// MSA client ID may have changed; re-validate the active session.
		return m, tea.Batch(bannerCmd, m.loadInstances(), m.sessionRecheckCmd())

	case ui.InstanceEdited:
		m.state = StateHome
		m.editInstance = nil
		if msg.Instance != nil {
			var bannerCmd tea.Cmd
			if err := m.instances.Update(msg.Instance); err != nil {
				bannerCmd = m.home.SetTransientBanner(fmt.Sprintf("Couldn't save instance: %v", err))
			} else {
				bannerCmd = m.home.SetTransientBanner("Instance saved.")
			}
			id := msg.Instance.ID
			return m, tea.Batch(bannerCmd, m.loadInstancesSelecting(id), m.sessionRecheckCmd())
		}
		return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())

	case ui.ModInstallDoneMsg:
		if m.mods != nil {
			newMods, cmd := m.mods.Update(msg)
			m.mods = newMods.(*ui.ModsModel)
			return m, cmd
		}
		return m, nil

	case ui.DeleteInstance:
		if running := m.runningInstance(); running != nil && msg.Instance != nil && msg.Instance.ID == running.ID {
			return m, m.home.SetTransientBanner(fmt.Sprintf("Can't delete %s while it's running.", running.Name))
		}
		if msg.Instance != nil {
			var bannerCmd tea.Cmd
			if err := m.instances.Delete(msg.Instance.ID); err != nil {
				bannerCmd = m.home.SetTransientBanner(fmt.Sprintf("Couldn't delete instance: %v", err))
			}
			return m, tea.Batch(bannerCmd, m.loadInstances())
		}
	// Instance management
	case ui.InstanceCreated:
		if err := m.instances.Create(msg.Instance); err != nil {
			m.state = StateHome
			bannerCmd := m.home.SetTransientBanner(fmt.Sprintf("Couldn't create instance: %v", err))
			return m, tea.Batch(bannerCmd, m.loadInstances(), m.sessionRecheckCmd())
		}
		m.state = StateHome
		id := msg.Instance.ID
		return m, tea.Batch(m.loadInstancesSelecting(id), m.sessionRecheckCmd())

	// Launch status updates - continue subscription
	case ui.LaunchStatusUpdate:
		if !m.isCurrentLaunch(msg.ID) {
			return m, nil // from a cancelled/replaced launch: drop, don't resubscribe
		}
		var cmd tea.Cmd
		if m.launch != nil {
			_, cmd = m.launch.Update(msg)
		}
		// Continue listening on this launch's channel
		return m, tea.Batch(cmd, m.waitForLaunchStatus(msg.ID, m.launchStatusChan))

	// Cancel launch - user pressed ESC (or confirmed "Stop game")
	case ui.CancelLaunch:
		if m.launchLive && m.launch != nil && m.launch.Playing() {
			// Stopping the running game: interrupt it, but keep the launch —
			// and the one-launch lock — until it has actually exited. It may
			// take a while to save, and a relaunch meanwhile would put two
			// games on one game dir.
			m.launchStopping = true
			if m.launchCtxCancel != nil {
				m.launchCtxCancel()
			}
			return m, nil
		}
		m.stopLaunch()
		m.launch = nil
		playingCmd := m.home.SetPlaying("")
		m.state = StateHome
		return m, tea.Batch(playingCmd, m.loadInstances(), m.sessionRecheckCmd())

	// Back home while the game keeps running.
	case ui.LeaveLaunchScreen:
		m.state = StateHome
		var playingCmd tea.Cmd
		if running := m.runningInstance(); running != nil {
			playingCmd = m.home.SetPlaying(running.ID)
		}
		return m, tea.Batch(playingCmd, m.loadInstances(), m.sessionRecheckCmd())

	// Launch complete - clean up
	case ui.LaunchComplete:
		if !m.isCurrentLaunch(msg.ID) {
			return m, nil
		}
		if m.launchStopping {
			msg.Error = nil // the interrupt exit the user asked for, not a crash
		}
		m.stopLaunch()
		playingCmd := m.home.SetPlaying("")
		if m.state != StateLaunch {
			// The game exited while the user was elsewhere: don't navigate
			// them; refresh (playtime) and report a crash.
			m.launch = nil
			if msg.Error != nil {
				// Persistent: the user may be on another screen right now.
				m.home.SetNotice(fmt.Sprintf("Minecraft exited with an error: %v", msg.Error))
			}
			return m, tea.Batch(playingCmd, m.loadInstances())
		}
		var cmd tea.Cmd
		if m.launch != nil {
			newLaunch, c := m.launch.Update(msg)
			m.launch = newLaunch.(*ui.LaunchModel)
			cmd = c
		}
		return m, tea.Batch(playingCmd, cmd)

	// Retry launch
	case ui.RetryLaunch:
		if m.launch == nil || m.launchLive || m.gatingLaunch {
			return m, nil
		}
		inst := m.launch.GetInstance()
		if msg.Offline {
			// A fresh screen: the failed one still has done/err set, which
			// would keep showing "Failed" and make esc skip the cancel.
			return m, m.openLaunchScreen(inst, true)
		}
		m.gatingLaunch = true
		m.home.SetLaunchGate(inst.Name)
		return m, m.gateOnlineLaunch(inst)

	// Global key handlers
	case tea.KeyMsg:
		armed := m.quitArmed
		m.quitArmed = false
		switch {
		case key.Matches(msg, m.keys.ForceQuit):
			return m, tea.Quit
		case key.Matches(msg, m.keys.Quit):
			// Don't quit on `q` while home owns input — an overlay (typing `q` in
			// the palette, or `q` cancelling a dialog) or the list filter (a name
			// containing "q").
			if m.state == StateHome && !m.home.CapturesInput() {
				if m.launchLive && !armed {
					// The game would keep running unmanaged (and its playtime
					// unrecorded): ask for a second q.
					m.quitArmed = true
					return m, m.home.SetTransientBanner("Minecraft is still running — press q again to quit (its playtime won't be recorded).")
				}
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
	case StateResourcePacks:
		if m.resourcePacks != nil {
			newRP, cmd := m.resourcePacks.Update(msg)
			m.resourcePacks = newRP.(*ui.ResourcePacksModel)
			cmds = append(cmds, cmd)
		}
	case StateSettings:
		if m.settings != nil {
			newSettings, cmd := m.settings.Update(msg)
			m.settings = newSettings.(*ui.SettingsModel)
			cmds = append(cmds, cmd)
		}
	case StateEditInstance:
		if m.editInstance != nil {
			newEdit, cmd := m.editInstance.Update(msg)
			m.editInstance = newEdit.(*ui.EditInstanceModel)
			cmds = append(cmds, cmd)
		}
	}

	return m, tea.Batch(cmds...)
}

// openLaunchScreen shows a fresh launch screen for inst and starts the launch.
func (m *Model) openLaunchScreen(inst *core.Instance, offline bool) tea.Cmd {
	m.state = StateLaunch
	m.launch = ui.NewLaunchModel(inst, m.cfg)
	cw, ch := m.contentSize()
	m.launch.SetSize(cw, ch)
	return tea.Batch(
		m.launch.Init(),
		m.beginLaunch(inst, offline),
	)
}

// runningInstance is the instance of the live launch, or nil.
func (m *Model) runningInstance() *core.Instance {
	if !m.launchLive || m.launch == nil {
		return nil
	}
	return m.launch.GetInstance()
}

// isCurrentLaunch reports whether a launch message belongs to the live launch.
func (m *Model) isCurrentLaunch(id int) bool {
	return m.launchLive && id == m.launchID
}

// stopLaunch cancels the live launch (if any) and forgets its channel. Its
// goroutine unwinds on the cancelled context and closes the channel; any
// messages still in flight carry the old ID and are dropped.
func (m *Model) stopLaunch() {
	if m.launchCtxCancel != nil {
		m.launchCtxCancel()
	}
	m.launchCtxCancel = nil
	m.launchStatusChan = nil
	m.launchLive = false
	m.launchStopping = false
}

// blockIfRunning refuses to open a screen that changes the running instance's
// files (the game holds them open, and rewrites options.txt when it exits).
func (m *Model) blockIfRunning(inst *core.Instance, what string) tea.Cmd {
	if running := m.runningInstance(); running != nil && inst != nil && inst.ID == running.ID {
		return m.home.SetTransientBanner(fmt.Sprintf("Close Minecraft before changing %s's %s.", running.Name, what))
	}
	return nil
}

// beginLaunch sets up the launch context and status channel on the event loop
// (so reads/writes of m.launchCtxCancel and m.launchStatusChan stay single-threaded),
// then hands them to startLaunch's command goroutine.
func (m *Model) beginLaunch(inst *core.Instance, offline bool) tea.Cmd {
	m.stopLaunch() // never leave a previous launch running unreachable
	m.launchID++
	m.launchLive = true
	ctx, cancel := context.WithCancel(context.Background())
	m.launchCtxCancel = cancel
	m.launchStatusChan = make(chan launch.Status, 10)

	// The launch screen's [v] and the launcher's log readers share this filter.
	logFilter := launch.NewLogFilter(launch.ParseLaunchLogVerbosity(m.cfg.LaunchLogVerbosity))
	if m.launch != nil {
		m.launch.SetLogFilter(logFilter)
	}

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

	return m.startLaunch(ctx, m.launchID, m.launchStatusChan, logFilter, inst, offline, playerName, uuid, accessToken)
}

func (m *Model) startLaunch(ctx context.Context, id int, statusChan chan launch.Status, logFilter *launch.LogFilter, inst *core.Instance, offline bool, playerName, uuid, accessToken string) tea.Cmd {
	// send delivers a status unless the launch was cancelled — after a cancel
	// nobody reads statusChan, so a plain send would block this goroutine forever.
	send := func(s launch.Status) {
		select {
		case statusChan <- s:
		case <-ctx.Done():
		}
	}
	return func() tea.Msg {
		// Find version info (vanilla or merged loader profile)
		details, err := loader.ResolveVersionDetails(ctx, m.mojang, inst, offline)
		if err != nil {
			return ui.LaunchComplete{ID: id, Error: err}
		}
		if loader.ParseKind(inst.Loader) == loader.KindFabric {
			_ = m.instances.Update(inst)
		}

		// Validate version info
		if details.MainClass == "" {
			return ui.LaunchComplete{ID: id, Error: fmt.Errorf("invalid version info: missing main class")}
		}

		// Player info (playerName/uuid/accessToken) is snapshotted by beginLaunch on
		// the event loop and passed in, so this goroutine never reads the shared
		// *core.Account that a session refresh may rewrite concurrently.

		// Start launcher in goroutine (starter Fabric mods first if requested, with progress on this screen)
		go func() {
			defer close(statusChan)
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
						send(launch.Status{
							Step:     "Installing starter mods",
							Message:  fmt.Sprintf("Downloading %s (%d/%d). Slow connections may take several minutes.", label, i+1, total),
							Progress: p,
						})
					})
					if err != nil {
						send(launch.Status{
							Step:    "Installing starter mods",
							Message: err.Error(),
							Error:   err,
						})
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
				LogFilter:        logFilter,
				UpdateLastPlayed: m.instances.UpdateLastPlayed,
				AddPlayTime:      m.instances.AddPlayTime,
				UpdateInstance:   m.instances.Update,
			}, statusChan)

			err := launcher.Launch(ctx)

			// Send final status; the deferred close ends the subscription.
			if err != nil {
				send(launch.Status{
					Step:    "Error",
					Message: err.Error(),
					Error:   err,
				})
			}
		}()

		// Return first status update command
		return m.waitForLaunchStatus(id, statusChan)()
	}
}

// waitForLaunchStatus creates a command that waits for the next status of
// launch id. The channel is captured by value so the command goroutine never
// reads the m.launchStatusChan field (which the event loop may nil on
// cancel/complete); every message carries id so stale ones can be dropped.
func (m *Model) waitForLaunchStatus(id int, ch chan launch.Status) tea.Cmd {
	return func() tea.Msg {
		if ch == nil {
			return ui.LaunchComplete{ID: id}
		}

		status, ok := <-ch
		if !ok {
			// Channel closed, launch complete
			return ui.LaunchComplete{ID: id}
		}

		if status.Error != nil {
			return ui.LaunchComplete{ID: id, Error: status.Error}
		}

		if status.IsComplete {
			return ui.LaunchComplete{ID: id}
		}

		return ui.LaunchStatusUpdate{ID: id, Status: status}
	}
}

// View implements tea.Model. All screens go through [ui.AppShellStyle] here only — do not pad in leaf views.
// The content is clamped to the drawable width as a defensive backstop so no leaf
// view can push a line past the frame (e.g. an unwrapped hint on a narrow terminal).
func (m *Model) View() string {
	content := m.shellContent()
	if cw, _ := m.contentSize(); cw > 0 {
		content = ui.ClampWidth(content, cw)
	}
	return ui.AppShellStyle.Background(ui.Active.Background).Render(content)
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
	case StateResourcePacks:
		if m.resourcePacks != nil {
			return m.resourcePacks.View()
		}
	case StateSettings:
		if m.settings != nil {
			return m.settings.View()
		}
	case StateEditInstance:
		if m.editInstance != nil {
			return m.editInstance.View()
		}
	}
	return "Unknown state"
}
