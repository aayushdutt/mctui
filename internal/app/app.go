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

	// Core services
	cfg       *config.Config
	instances *core.InstanceManager
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
	Quit key.Binding
	Help key.Binding
	Back key.Binding
}

func defaultKeyMap() keyMap {
	return keyMap{
		Quit: key.NewBinding(
			key.WithKeys("q", "ctrl+c"),
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

	return &Model{
		state:     StateHome,
		home:      ui.NewHomeModel(),
		cfg:       cfg,
		instances: instances,
		mojang:    api.NewMojangClient(),
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
		m.wizard = ui.NewWizardModel()
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
			m.startLaunch(msg.Instance),
		)

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

	// Launch complete - clean up
	case ui.LaunchComplete:
		if m.launch != nil {
			m.launch.Update(msg)
		}
		// Clean up
		m.launchStatusChan = nil
		if m.launchCtxCancel != nil {
			m.launchCtxCancel = nil
		}
		return m, nil

	// Global key handlers
	case tea.KeyMsg:
		switch {
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

	case StateLaunch:
		if m.launch != nil {
			newLaunch, cmd := m.launch.Update(msg)
			m.launch = newLaunch.(*ui.LaunchModel)
			cmds = append(cmds, cmd)
		}
	}

	return m, tea.Batch(cmds...)
}

func (m *Model) startLaunch(inst *core.Instance) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithCancel(context.Background())
		m.launchCtxCancel = cancel

		// Find version info
		version, err := m.mojang.FindVersion(ctx, inst.Version)
		if err != nil {
			return ui.LaunchComplete{Error: err}
		}

		details, err := m.mojang.GetVersionDetails(ctx, version)
		if err != nil {
			return ui.LaunchComplete{Error: err}
		}

		// Validate version info
		if details.MainClass == "" {
			return ui.LaunchComplete{Error: fmt.Errorf("invalid version info: missing main class")}
		}

		// Create status channel
		m.launchStatusChan = make(chan launch.Status, 10)

		// Start launcher in goroutine
		go func() {
			launcher := launch.NewLauncher(&launch.Options{
				Instance:    inst,
				VersionInfo: details,
				Offline:     true,
				PlayerName:  "Player",
				Config:      m.cfg,
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
	}

	return "Unknown state"
}
