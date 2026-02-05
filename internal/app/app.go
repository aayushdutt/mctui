// Package app contains the main Bubbletea application model.
// This is the central hub that manages app state and delegates to child views.
package app

import (
	"context"
	"fmt"

	"github.com/charmbracelet/bubbles/key"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/quasar/mctui/internal/api"
	"github.com/quasar/mctui/internal/config"
	"github.com/quasar/mctui/internal/core"
	"github.com/quasar/mctui/internal/launch"
	"github.com/quasar/mctui/internal/ui"
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
	auth   *ui.AuthModel

	// Core services
	cfg       *config.Config
	instances *core.InstanceManager
	accounts  *core.AccountManager
	mojang    *api.MojangClient

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
		keys:      defaultKeyMap(),
	}
}

// Init implements tea.Model
func (m *Model) Init() tea.Cmd {
	return tea.Batch(
		m.home.Init(),
		m.loadInstances(),
	)
}

func (m *Model) loadInstances() tea.Cmd {
	return func() tea.Msg {
		err := m.instances.Load()
		return ui.InstancesLoaded{
			Instances: m.instances.List(),
			Error:     err,
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

		// Propagate size to child models
		m.home.SetSize(msg.Width, msg.Height)
		if m.wizard != nil {
			m.wizard.SetSize(msg.Width, msg.Height)
		}
		if m.launch != nil {
			m.launch.SetSize(msg.Width, msg.Height)
		}

	// Navigation messages
	case ui.NavigateToHome:
		m.state = StateHome
		return m, m.loadInstances()

	case ui.NavigateToNewInstance:
		m.state = StateNewInstance
		m.wizard = ui.NewWizardModel(m.instances.List())
		m.wizard.SetSize(m.width, m.height)
		return m, tea.Batch(
			m.wizard.Init(),
			m.loadVersions(),
		)

	case ui.NavigateToLaunch:
		m.state = StateLaunch
		m.launch = ui.NewLaunchModel(msg.Instance)
		m.launch.SetSize(m.width, m.height)
		return m, tea.Batch(
			m.launch.Init(),
			m.startLaunch(msg.Instance, msg.Offline),
		)

	case ui.NavigateToAuth:
		m.state = StateAuth
		clientID := m.cfg.MSAClientID
		if clientID == "" {
			// Fallback or error? For now use a placeholder to allow testing
			clientID = "YOUR_CLIENT_ID"
		}
		m.auth = ui.NewAuthModel(m.cfg.DataDir, clientID, m.accounts)
		m.auth.SetSize(m.width, m.height)
		return m, m.auth.Init()

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
		return m, m.loadInstances()

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
		return m, m.loadInstances()

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
			return m, m.startLaunch(inst, msg.Offline)
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
	}

	return m, tea.Batch(cmds...)
}

func (m *Model) startLaunch(inst *core.Instance, offline bool) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithCancel(context.Background())
		m.launchCtxCancel = cancel

		// Find version info
		details, err := m.mojang.ResolveVersionDetails(ctx, inst.Version, offline)
		if err != nil {
			return ui.LaunchComplete{Error: err}
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

		// Start launcher in goroutine
		go func() {
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

// View implements tea.Model
func (m *Model) View() string {
	if !m.ready {
		return "Initializing..."
	}

	// Delegate to current view
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
	}

	return "Unknown state"
}
