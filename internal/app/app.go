// Package app contains the main Bubbletea application model.
// This is the central hub that manages app state and delegates to child views.
package app

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/charmbracelet/bubbles/key"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/mctui/mctui/internal/api"
	"github.com/mctui/mctui/internal/config"
	"github.com/mctui/mctui/internal/core"
	"github.com/mctui/mctui/internal/launch"
	"github.com/mctui/mctui/internal/loader"
	"github.com/mctui/mctui/internal/mods"
	"github.com/mctui/mctui/internal/ui"
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
	home   *ui.HomeModel
	wizard *ui.WizardModel
	launch *ui.LaunchModel
	mods   *ui.ModsModel
	auth   *ui.AuthModel

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
	cfg.EnsureDirs()

	instances := core.NewInstanceManager(cfg.DataDir)
	accounts := core.NewAccountManager(cfg.DataDir)
	accounts.Load()

	home := ui.NewHomeModel()
	home.SetAccountManager(accounts)

	return &Model{
		state:     StateHome,
		home:      home,
		cfg:       cfg,
		instances: instances,
		accounts:  accounts,
		mojang:    api.NewMojangClient(cfg.DataDir),
		modrinth:  api.NewModrinthClient(),
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

func (m *Model) validateMSAccessToken(ctx context.Context, acc *core.Account) error {
	return api.NewAuthClient(m.effectiveMSAClientID()).ValidateMinecraftToken(ctx, acc.AccessToken)
}

func (m *Model) checkActiveSessionCmd() tea.Cmd {
	return func() tea.Msg {
		acc := m.accounts.GetActive()
		if acc == nil || acc.Type != core.AccountTypeMSA {
			return ui.ActiveSessionCheckResult{Status: ui.ActiveSessionNotApplicable}
		}
		if acc.IsExpired() {
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

	// Navigation messages
	case ui.NavigateToHome:
		if m.mods != nil {
			m.mods.CancelPending()
		}
		m.state = StateHome
		m.mods = nil
		return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())

	case ui.NavigateToSettings:
		m.state = StateSettings
		cw, ch := m.contentSize()
		m.home.SetSize(cw, ch)
		return m, nil

	case ui.NavigateToNewInstance:
		m.state = StateNewInstance
		m.wizard = ui.NewWizardModel(m.instances.List())
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
				m.startLaunch(msg.Instance, true),
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
			m.startLaunch(msg.Instance, false),
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

	case ui.NavigateToAuth:
		return m, m.prepareAuthScreen()

	case ui.ModInstallDoneMsg:
		if m.mods != nil {
			newMods, cmd := m.mods.Update(msg)
			m.mods = newMods.(*ui.ModsModel)
			return m, cmd
		}
		return m, nil

	case ui.DeleteInstance:
		if msg.Instance != nil {
			_ = m.instances.Delete(msg.Instance.ID)
			return m, m.loadInstances()
		}
	// Instance management
	case ui.InstanceCreated:
		if err := m.instances.Create(msg.Instance); err != nil {
			// TODO: Show error
			return m, nil
		}
		m.state = StateHome
		id := msg.Instance.ID
		return m, tea.Batch(m.loadInstancesSelecting(id), m.sessionRecheckCmd())

	// Launch status updates - continue subscription
	case ui.LaunchStatusUpdate:
		if m.launch != nil {
			m.launch.Update(msg)
		}
		// Continue listening for more status updates
		return m, m.waitForLaunchStatus()

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
				return m, m.startLaunch(inst, true)
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
		case key.Matches(msg, m.keys.Back):
			if m.state == StateSettings {
				m.state = StateHome
				return m, tea.Batch(m.loadInstances(), m.sessionRecheckCmd())
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
	}

	return m, tea.Batch(cmds...)
}

func (m *Model) startLaunch(inst *core.Instance, offline bool) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithCancel(context.Background())
		m.launchCtxCancel = cancel

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

		// Create status channel
		m.launchStatusChan = make(chan launch.Status, 10)

		// Determine player info
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
						m.launchStatusChan <- launch.Status{
							Step:     "Installing starter mods",
							Message:  fmt.Sprintf("Downloading %s (%d/%d). Slow connections may take several minutes.", label, i+1, total),
							Progress: p,
						}
					})
					if err != nil {
						m.launchStatusChan <- launch.Status{
							Step:    "Installing starter mods",
							Message: err.Error(),
							Error:   err,
						}
						close(m.launchStatusChan)
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
			}, m.launchStatusChan)

			err := launcher.Launch(ctx)

			// Send final status then close
			if err != nil {
				m.launchStatusChan <- launch.Status{
					Step:    "Error",
					Message: err.Error(),
					Error:   err,
				}
			}
			close(m.launchStatusChan)
		}()

		// Return first status update command
		return m.waitForLaunchStatus()()
	}
}

// waitForLaunchStatus creates a command that waits for the next launch status
func (m *Model) waitForLaunchStatus() tea.Cmd {
	return func() tea.Msg {
		if m.launchStatusChan == nil {
			return ui.LaunchComplete{}
		}

		status, ok := <-m.launchStatusChan
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
	return ui.AppShellStyle.Render(m.shellContent())
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
		return "Settings — coming soon\n\n[esc] Back to home"
	}
	return "Unknown state"
}
