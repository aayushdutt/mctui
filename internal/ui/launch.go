// Package ui provides the launch progress view.
package ui

import (
	"fmt"
	"strings"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/launch"
	"github.com/charmbracelet/bubbles/progress"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
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
	logs     []string

	cfg *config.Config // optional; used for log verbosity while playing

	palette CommandPalette

	// help is the `?` read-only key reference overlay (kit_help.go), dismissed by
	// ? / esc / q.
	help bool
}

// helpGroups returns the launch screen's bindings grouped for the `?` overlay,
// hand-built to match exactly what Update handles (retry/offline on failure,
// log verbosity while playing, esc cancel/back, force quit) and the footers.
func (m *LaunchModel) helpGroups() []HelpGroup {
	// State-aware so the overlay never advertises a key that is a no-op right now:
	// retry/offline only exist after a failure, enter only navigates once done,
	// and v only cycles verbosity while the game is running.
	var act []HelpEntry
	escDesc := "cancel launch"
	switch {
	case m.done && m.err != nil:
		act = []HelpEntry{{"enter", "back home"}, {"r", "retry"}, {"o", "retry offline"}}
		escDesc = "back home"
	case m.done:
		act = []HelpEntry{{"enter", "back home"}}
		escDesc = "back home"
	case m.status.Step == "Playing":
		act = []HelpEntry{{"v", "cycle log verbosity"}}
	}

	groups := make([]HelpGroup, 0, 2)
	if len(act) > 0 {
		groups = append(groups, HelpGroup{Title: "Act", Entries: act})
	}
	groups = append(groups, HelpGroup{Title: "General", Entries: []HelpEntry{
		{":", "command palette"},
		{"?", "help"},
		{"esc", escDesc},
		{"ctrl+c", "force quit"},
	}})
	return groups
}

type stepInfo struct {
	name   string
	status string // pending, running, done, error
}

// advancementCopy maps each technical launch step to Minecraft advancement-style
// flavor text (Phase 3's launch personality). The technical name is kept in the
// running row so the line still says what is really happening; unmapped steps
// (e.g. "Installing starter mods") fall back to their plain name.
var advancementCopy = map[string]string{
	"Checking Java":         "Java acquired",
	"Downloading libraries": "Getting an upgrade",
	"Downloading assets":    "Punching trees…",
	"Preparing game":        "Ready to spawn",
	"Launching":             "Entering world…",
}

// stepRowText renders one step's line text. Done steps read as an unlocked
// advancement; the running step shows the advancement with its technical name in
// parentheses; errors stay plain (never joke in an error).
func stepRowText(step stepInfo) string {
	adv := advancementCopy[step.name]
	if adv == "" {
		adv = step.name
	}
	switch step.status {
	case "done":
		return "Advancement made: " + adv
	case "running":
		if adv == step.name {
			return adv
		}
		return adv + " (" + strings.ToLower(step.name) + ")"
	case "error":
		return step.name
	default:
		return adv
	}
}

// xpBar renders the launch progress as Minecraft's experience bar: a thin
// segmented ▰▱ track, Success-green fill, with the percentage flush right. Themed
// (Active.Success / BorderFaint) so it reads correctly in all six palettes.
func xpBar(percent float64, width int) string {
	if percent < 0 {
		percent = 0
	}
	if percent > 1 {
		percent = 1
	}
	if width < 8 {
		width = 8
	}
	pctStr := fmt.Sprintf(" %3.0f%%", percent*100)
	barW := width - lipgloss.Width(pctStr)
	if barW < 1 {
		barW = 1
	}
	filled := int(percent*float64(barW) + 0.5)
	if filled > barW {
		filled = barW
	}
	fill := lipgloss.NewStyle().Foreground(Active.Success).Render(strings.Repeat("▰", filled))
	rest := lipgloss.NewStyle().Foreground(Active.BorderFaint).Render(strings.Repeat("▱", barW-filled))
	pct := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(pctStr)
	return fill + rest + pct
}

// NewLaunchModel creates a new launch view. cfg is used for game log verbosity ([v] while playing).
func NewLaunchModel(instance *core.Instance, cfg *config.Config) *LaunchModel {
	p := ThemeProgress(50)

	m := &LaunchModel{
		instance: instance,
		cfg:      cfg,
		progress: p,
		steps: []stepInfo{
			{name: "Checking Java", status: "pending"},
			{name: "Downloading libraries", status: "pending"},
			{name: "Downloading assets", status: "pending"},
			{name: "Preparing game", status: "pending"},
			{name: "Launching", status: "pending"},
		},
	}
	m.palette = NewCommandPalette("Launch · commands", "Run a command…", m.launchPaletteItems)
	return m
}

// leaveLaunch returns the command for leaving the launch screen. A completed
// launch just navigates home, but an IN-PROGRESS one must cancel first
// (CancelLaunch tears down the launch context + status channel in app.go); a
// plain NavigateToHome would leave the launch goroutine running and the context
// leaked. Shared by the esc key and the palette "Back to home" verb so both
// paths clean up identically.
func (m *LaunchModel) leaveLaunch() tea.Cmd {
	if m.done {
		return func() tea.Msg { return NavigateToHome{} }
	}
	m.status.Message = "Cancelling..."
	return func() tea.Msg { return CancelLaunch{} }
}

// launchPaletteItems supplies the launch screen's `:` command-palette verbs.
// Retry is offered only once a launch has failed.
func (m *LaunchModel) launchPaletteItems() []PaletteItem {
	leaveLabel := "Back to home"
	if !m.done {
		leaveLabel = "Cancel launch"
	}
	items := []PaletteItem{
		{Label: leaveLabel, Run: m.leaveLaunch},
	}
	if m.done && m.err != nil {
		items = append(items, PaletteItem{Label: "Retry", Run: func() tea.Cmd {
			return func() tea.Msg { return RetryLaunch{Offline: false} }
		}})
	}
	return items
}

// SetSize updates dimensions
func (m *LaunchModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.progress.Width = width - 10
}

// GetInstance returns the instance being launched
func (m *LaunchModel) GetInstance() *core.Instance {
	return m.instance
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

		if msg.Status.LogLine != nil {
			line := fmt.Sprintf("[%s] %s", msg.Status.LogLine.Type, msg.Status.LogLine.Text)
			m.logs = append(m.logs, line)
			if len(m.logs) > 15 {
				m.logs = m.logs[len(m.logs)-15:]
			}
		}

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
			// Auto return to home immediately
			return m, func() tea.Msg { return NavigateToHome{} }
		}
		return m, nil

	case progress.FrameMsg:
		progressModel, cmd := m.progress.Update(msg)
		m.progress = progressModel.(progress.Model)
		return m, cmd

	case tea.KeyMsg:
		if m.palette.Active {
			return m, m.palette.Update(msg)
		}
		// Help overlay is read-only: it swallows every key, and only ? / esc / q
		// dismiss it.
		if m.help {
			switch msg.String() {
			case "?", "esc", "q":
				m.help = false
			}
			return m, nil
		}
		switch msg.String() {
		case ":":
			m.palette.Open()
			return m, textinput.Blink
		case "?":
			// No text input on this screen, so "?" always opens the key reference.
			m.help = true
			return m, nil
		case "ctrl+c":
			return m, tea.Quit
		case "esc", "q":
			// Same teardown as the palette "Cancel launch" / "Back to home" verb.
			return m, m.leaveLaunch()
		case "enter":
			if m.done {
				return m, func() tea.Msg { return NavigateToHome{} }
			}
		case "r":
			if m.done && m.err != nil {
				return m, func() tea.Msg { return RetryLaunch{Offline: false} }
			}
		case "o":
			if m.done && m.err != nil {
				return m, func() tea.Msg { return RetryLaunch{Offline: true} }
			}
		case "v":
			if m.cfg != nil && m.status.Step == "Playing" && !m.done {
				m.cfg.LaunchLogVerbosity = launch.CycleLaunchLogVerbosity(m.cfg.LaunchLogVerbosity)
				_ = m.cfg.Save()
			}
		}
	}

	return m, nil
}

func (m *LaunchModel) updateSteps() {
	// Dynamically add Installing starter mods (Fabric bundle before the normal pipeline)
	if m.status.Step == "Installing starter mods" {
		found := false
		for _, s := range m.steps {
			if s.name == "Installing starter mods" {
				found = true
				break
			}
		}
		if !found {
			newSteps := append([]stepInfo{{name: "Installing starter mods", status: "pending"}}, m.steps...)
			m.steps = newSteps
		}
	}

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
	if m.help {
		return boundToScreen(HelpOverlay("Launch", m.helpGroups(), m.width, m.height), m.width, m.height)
	}
	if m.palette.Active {
		return boundToScreen(m.palette.View(m.width, m.height), m.width, m.height)
	}

	// Panel width: derive from content width, clamp to a readable band and
	// guard against an unset (0) width early in the lifecycle. The progress bar
	// (width 50) lives just above the panels, so keep panels at least 54 wide.
	panelW := m.width
	if panelW > 60 {
		panelW = 60
	}
	if panelW < 54 {
		panelW = 54
	}

	// Header: filled accent pill (kept) + instance info line.
	headerStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(OnColor(Active.Primary)).
		Background(Active.Primary).
		Padding(0, 1)

	headerText := fmt.Sprintf("Launching: %s", m.instance.Name)
	if m.status.Step == "Playing" {
		headerText = fmt.Sprintf("Playing: %s", m.instance.Name)
	}
	header := headerStyle.Render(headerText)

	info := lipgloss.NewStyle().
		Foreground(Active.TextSubtle).
		Render(fmt.Sprintf("Minecraft %s  %s  %s", m.instance.Version, GlyphDot, m.instance.Loader))

	// Progress panel: the bar, then the step rows. The currently-running step is
	// emphasized bold; done steps are success, pending steps dim.
	var steps strings.Builder
	for i, step := range m.steps {
		var icon string
		var style lipgloss.Style
		switch step.status {
		case "done":
			icon = GlyphDone
			style = lipgloss.NewStyle().Foreground(Active.Success)
		case "running":
			icon = GlyphRunning
			style = lipgloss.NewStyle().Bold(true).Foreground(Active.WarningStrong)
		case "error":
			icon = GlyphFail
			style = lipgloss.NewStyle().Foreground(Active.Error)
		default:
			icon = GlyphPending
			style = lipgloss.NewStyle().Foreground(Active.TextFaint)
		}
		if i > 0 {
			steps.WriteString("\n")
		}
		steps.WriteString(style.Render(icon + " " + stepRowText(step)))
	}

	// The progress bar renders at its own fixed width, so keep it above the
	// panel rather than inside (where the bar + percentage would wrap).
	stepsPanel := Zone("Progress", steps.String(), panelW, true)

	// Status message under the progress panel.
	statusMsg := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(m.status.Message)

	// Completion / error footer or in-flight key hints, all routed via KeyHints.
	var footer string
	if m.done {
		if m.err != nil {
			fail := lipgloss.NewStyle().
				Bold(true).
				Foreground(Active.Error).
				Render(fmt.Sprintf("%s Failed: %v", GlyphFail, m.err))
			hints := KeyHints(panelW,
				KeyHint{"r", "retry"},
				KeyHint{"o", "offline mode"},
				KeyHint{"enter", "home"},
				KeyHint{"?", "help"},
			)
			footer = lipgloss.JoinVertical(lipgloss.Left, fail, "", hints)
		} else {
			footer = lipgloss.NewStyle().
				Bold(true).
				Foreground(Active.Success).
				Render(fmt.Sprintf("%s Game closed. Returning to home…", GlyphDone))
		}
	} else if m.status.Step == "Playing" {
		if m.cfg != nil {
			v := launch.ParseLaunchLogVerbosity(m.cfg.LaunchLogVerbosity)
			footer = KeyHints(panelW,
				KeyHint{"ctrl+c", "force quit"},
				KeyHint{"v", "logs: " + v.ShortLabel()},
				KeyHint{"?", "help"},
			)
		} else {
			footer = KeyHints(panelW, KeyHint{"ctrl+c", "force quit"})
		}
	} else {
		footer = KeyHints(panelW,
			KeyHint{"esc", "cancel"},
			KeyHint{"ctrl+c", "quit"},
			KeyHint{"?", "help"},
		)
	}

	// Assemble: gaps are explicit "" entries rather than embedded "\n". The
	// progress model still drives the animation; xpBar renders its % as the
	// Minecraft experience bar.
	parts := []string{
		header,
		info,
		"",
		xpBar(m.progress.Percent(), m.progress.Width),
		"",
		stepsPanel,
	}
	if m.status.Message != "" {
		parts = append(parts, "", statusMsg)
	}

	// Logs panel (only when there are logs).
	if len(m.logs) > 0 {
		logStyle := lipgloss.NewStyle().Foreground(Active.TextFaint)
		styled := make([]string, len(m.logs))
		for i, line := range m.logs {
			styled[i] = logStyle.Render(line)
		}
		logsPanel := Zone("Logs", strings.Join(styled, "\n"), panelW, false)
		parts = append(parts, "", logsPanel)
	}

	parts = append(parts, "", footer)

	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}
