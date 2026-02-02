// Package ui provides the launch progress view.
package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/progress"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/quasar/mctui/internal/core"
	"github.com/quasar/mctui/internal/launch"
)

// LaunchModel shows launch progress
type LaunchModel struct {
	instance *core.Instance
	width    int
	height   int

	progress progress.Model
	status   launch.Status
	steps    []stepInfo
	done     bool
	err      error
}

type stepInfo struct {
	name   string
	status string // pending, running, done, error
}

// NewLaunchModel creates a new launch view
func NewLaunchModel(instance *core.Instance) *LaunchModel {
	p := progress.New(
		progress.WithDefaultGradient(),
		progress.WithWidth(50),
	)

	return &LaunchModel{
		instance: instance,
		progress: p,
		steps: []stepInfo{
			{name: "Checking Java", status: "pending"},
			{name: "Downloading libraries", status: "pending"},
			{name: "Downloading assets", status: "pending"},
			{name: "Preparing game", status: "pending"},
			{name: "Launching", status: "pending"},
		},
	}
}

// SetSize updates dimensions
func (m *LaunchModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.progress.Width = width - 10
}

// Init implements tea.Model
func (m *LaunchModel) Init() tea.Cmd {
	return nil
}

// Update implements tea.Model
func (m *LaunchModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case LaunchStatusUpdate:
		m.status = msg.Status
		m.updateSteps()

		// Update progress bar
		cmd := m.progress.SetPercent(msg.Status.Progress)
		return m, cmd

	case LaunchComplete:
		m.done = true
		m.err = msg.Error
		if msg.Error != nil {
			m.updateStepStatus(m.status.Step, "error")
		} else {
			m.updateStepStatus(m.status.Step, "done")
		}
		return m, nil

	case progress.FrameMsg:
		progressModel, cmd := m.progress.Update(msg)
		m.progress = progressModel.(progress.Model)
		return m, cmd

	case tea.KeyMsg:
		switch msg.String() {
		case "ctrl+c":
			return m, tea.Quit
		case "esc", "q":
			if m.done {
				return m, func() tea.Msg { return NavigateToHome{} }
			}
			// TODO: Cancel launch
		case "enter":
			if m.done {
				return m, func() tea.Msg { return NavigateToHome{} }
			}
		}
	}

	return m, nil
}

func (m *LaunchModel) updateSteps() {
	// Dynamically add Downloading Java if it occurs
	if m.status.Step == "Downloading Java" {
		found := false
		for _, s := range m.steps {
			if s.name == "Downloading Java" {
				found = true
				break
			}
		}
		if !found {
			// Insert after Checking Java
			newSteps := make([]stepInfo, 0, len(m.steps)+1)
			for _, s := range m.steps {
				newSteps = append(newSteps, s)
				if s.name == "Checking Java" {
					newSteps = append(newSteps, stepInfo{name: "Downloading Java", status: "pending"})
				}
			}
			m.steps = newSteps
		}
	}

	for i := range m.steps {
		if m.steps[i].name == m.status.Step {
			m.steps[i].status = "running"
		} else if m.steps[i].status == "running" {
			m.steps[i].status = "done"
		}
	}
}

func (m *LaunchModel) updateStepStatus(stepName, status string) {
	for i := range m.steps {
		if m.steps[i].name == stepName {
			m.steps[i].status = status
			return
		}
	}
}

// View implements tea.Model
func (m *LaunchModel) View() string {
	// ... (Header and Info same as replace target or keep?)
	// I need to be careful with replace range.
	// The target range is from line 91 (Update switch) to end of View.
	// I'll rewrite View mostly to be safe.

	headerStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FAFAFA")).
		Background(lipgloss.Color("#7C3AED")).
		Padding(0, 1)

	header := headerStyle.Render(fmt.Sprintf("Launching: %s", m.instance.Name))

	// Instance info
	info := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#A1A1AA")).
		Render(fmt.Sprintf("Minecraft %s • %s", m.instance.Version, m.instance.Loader))

	// Progress bar
	progressView := m.progress.View()

	// Steps
	var stepsView strings.Builder
	for _, step := range m.steps {
		var icon string
		var style lipgloss.Style
		switch step.status {
		case "done":
			icon = "✓"
			style = lipgloss.NewStyle().Foreground(lipgloss.Color("#10B981"))
		case "running":
			icon = "◐"
			style = lipgloss.NewStyle().Foreground(lipgloss.Color("#F59E0B"))
		case "error":
			icon = "✗"
			style = lipgloss.NewStyle().Foreground(lipgloss.Color("#EF4444"))
		default:
			icon = "○"
			style = lipgloss.NewStyle().Foreground(lipgloss.Color("#626262"))
		}
		stepsView.WriteString(style.Render(fmt.Sprintf("%s %s", icon, step.name)))
		stepsView.WriteString("\n")
	}

	// Status message
	msgStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#A1A1AA"))
	statusMsg := msgStyle.Render(m.status.Message)

	// Error or completion
	var footer string
	if m.done {
		if m.err != nil {
			footer = lipgloss.NewStyle().
				Foreground(lipgloss.Color("#EF4444")).
				Render(fmt.Sprintf("\n✗ Failed: %v\n\nPress Enter to go back", m.err))
		} else {
			footer = lipgloss.NewStyle().
				Foreground(lipgloss.Color("#10B981")).
				Render("\n✓ Game launched!\n\nPress Enter to go back")
		}
	} else {
		footer = lipgloss.NewStyle().
			Foreground(lipgloss.Color("#626262")).
			Render("\n[Esc] Cancel • [Ctrl+C] Quit")
	}

	return lipgloss.JoinVertical(
		lipgloss.Left,
		header,
		info,
		"",
		progressView,
		"",
		stepsView.String(),
		"",
		statusMsg,
		footer,
	)
}
