// Package ui provides the launch progress view.
package ui

import (
	"fmt"
	"strings"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/launch"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// LaunchModel shows launch progress
type LaunchModel struct {
	instance *core.Instance
	width    int
	height   int

	percent float64 // overall progress 0..1; only moves forward
	status  launch.Status
	steps   []stepInfo
	done    bool
	err     error
	logs    []string

	cfg *config.Config // optional; used for log verbosity while playing

	// logFilter is the running launcher's live log verbosity (see
	// SetLogFilter); [v] updates it so the change applies to the next line.
	logFilter *launch.LogFilter

	palette CommandPalette

	// help is the `?` read-only key reference overlay (kit_help.go), dismissed by
	// ? / esc / q.
	help bool

	// confirmStop shows the "Stop Minecraft?" dialog; stopFocusYes is its
	// highlighted option (Cancel by default — stopping can lose unsaved progress).
	confirmStop  bool
	stopFocusYes bool
	stopping     bool // "Stop game" confirmed; waiting for the game to exit
}

// playing reports whether the game process is running.
func (m *LaunchModel) playing() bool {
	return !m.done && m.status.Step == "Playing"
}

// Playing reports whether the game process is running (for the app: stopping
// it waits for the exit instead of tearing the launch down).
func (m *LaunchModel) Playing() bool {
	return m.playing()
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
	case m.playing():
		act = []HelpEntry{{"s", "stop game"}, {"v", "cycle log verbosity"}}
		escDesc = "home (game keeps running)"
	}

	groups := make([]HelpGroup, 0, 2)
	if len(act) > 0 {
		groups = append(groups, HelpGroup{Title: "Act", Entries: act})
	}
	groups = append(groups, HelpGroup{Title: "General", Entries: []HelpEntry{
		{":", "command palette"},
		{"?", "help"},
		{"esc", escDesc},
		{"ctrl+c", "quit mctui"},
	}})
	return groups
}

type stepInfo struct {
	name   string
	status string // pending, running, done, error
}

// stepLabel is the text for a launch step row: its technical name, except the
// final step reads as what the user is waiting for.
func stepLabel(step stepInfo) string {
	if step.name == "Launching" {
		return "Starting Minecraft"
	}
	return step.name
}

// progressBar renders overall progress as a thin line — a green run for the
// done part over a faint track — with the percentage flush right:
//
//	━━━━━━━━━━━━━━──────────────────────  46%
func progressBar(percent float64, width int) string {
	percent = min(1, max(0, percent))
	width = max(8, width)
	pct := fmt.Sprintf("  %3.0f%%", percent*100)
	barW := max(1, width-lipgloss.Width(pct))
	filled := min(barW, int(percent*float64(barW)+0.5))
	return lipgloss.NewStyle().Foreground(Active.Success).Render(strings.Repeat("━", filled)) +
		lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render(strings.Repeat("─", barW-filled)) +
		lipgloss.NewStyle().Foreground(Active.TextDim).Render(pct)
}

// updatePercent folds a status into the overall progress. The launcher reports
// a download step's own 0..1 fraction (and 0 on log lines), so the overall
// figure is (steps before + fraction of this one) / steps, never moving back,
// and 100% once the game is running.
func (m *LaunchModel) updatePercent(st launch.Status) {
	if st.Step == "Playing" {
		m.percent = 1
		return
	}
	if st.LogLine != nil || len(m.steps) == 0 {
		return
	}
	idx := -1
	for i, s := range m.steps {
		if s.name == st.Step {
			idx = i
			break
		}
	}
	if idx < 0 {
		return
	}
	frac := 0.0
	if strings.HasPrefix(st.Step, "Downloading") {
		frac = min(1, max(0, st.Progress))
	}
	if p := (float64(idx) + frac) / float64(len(m.steps)); p > m.percent {
		m.percent = p
	}
}

// NewLaunchModel creates a new launch view. cfg is used for game log verbosity ([v] while playing).
func NewLaunchModel(instance *core.Instance, cfg *config.Config) *LaunchModel {
	m := &LaunchModel{
		instance: instance,
		cfg:      cfg,
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
//
// While the game is running, leaving does NOT stop it: it returns home and the
// game keeps running (stopping is the separate, confirmed "Stop game").
func (m *LaunchModel) leaveLaunch() tea.Cmd {
	if m.done {
		return func() tea.Msg { return NavigateToHome{} }
	}
	if m.playing() {
		return func() tea.Msg { return LeaveLaunchScreen{} }
	}
	m.status.Message = "Cancelling..."
	return func() tea.Msg { return CancelLaunch{} }
}

// askStopGame opens the "Stop Minecraft?" confirmation.
func (m *LaunchModel) askStopGame() tea.Cmd {
	if !m.playing() || m.stopping {
		return nil
	}
	m.confirmStop = true
	m.stopFocusYes = false
	return nil
}

// updateStopDialog handles keys while the stop confirmation is open.
func (m *LaunchModel) updateStopDialog(msg tea.KeyMsg) tea.Cmd {
	if ConfirmKeyToggles(msg.String()) {
		m.stopFocusYes = !m.stopFocusYes
		return nil
	}
	stop := false
	switch key := msg.String(); {
	case key == "y" || key == "Y":
		stop = true
	case key == "enter":
		stop = m.stopFocusYes
	case ConfirmCancels(key):
	default:
		return nil
	}
	m.confirmStop = false
	if !stop || !m.playing() {
		return nil
	}
	m.stopping = true
	return func() tea.Msg { return CancelLaunch{} }
}

// launchPaletteItems supplies the launch screen's `:` command-palette verbs.
// Retry is offered only once a launch has failed.
func (m *LaunchModel) launchPaletteItems() []PaletteItem {
	leaveLabel := "Back to home"
	switch {
	case m.playing():
		leaveLabel = "Back to home (game keeps running)"
	case !m.done:
		leaveLabel = "Cancel launch"
	}
	items := []PaletteItem{
		{Label: leaveLabel, Run: m.leaveLaunch},
	}
	if m.playing() {
		items = append(items, PaletteItem{Label: "Stop game", Run: m.askStopGame})
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
}

// SetLogFilter connects [v] to the running launcher's log filter.
func (m *LaunchModel) SetLogFilter(f *launch.LogFilter) {
	m.logFilter = f
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
			line := msg.Status.LogLine.Text
			m.logs = append(m.logs, line)
			if len(m.logs) > 15 {
				m.logs = m.logs[len(m.logs)-15:]
			}
		}

		m.updatePercent(msg.Status)
		return m, nil

	case LaunchComplete:
		m.confirmStop = false // the game is gone; don't leave "Stop?" over the result
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

	case tea.KeyMsg:
		if m.palette.Active {
			return m, m.palette.Update(msg)
		}
		if m.confirmStop {
			return m, m.updateStopDialog(msg)
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
		case "esc":
			// Same path as the palette "Cancel launch" / "Back to home" verb.
			return m, m.leaveLaunch()
		case "s":
			if m.playing() {
				return m, m.askStopGame()
			}
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
			if m.cfg != nil && m.playing() {
				m.cfg.LaunchLogVerbosity = launch.CycleLaunchLogVerbosity(m.cfg.LaunchLogVerbosity)
				m.logFilter.Set(launch.ParseLaunchLogVerbosity(m.cfg.LaunchLogVerbosity))
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
	base := m.baseView()
	switch {
	case m.help:
		return Overlay(base, HelpCard("Launch", m.helpGroups(), m.width, m.height), m.width, m.height)
	case m.palette.Active:
		return Overlay(base, m.palette.View(m.width, m.height), m.width, m.height)
	case m.confirmStop:
		return Overlay(base, ConfirmDialog{
			Title:    "Stop Minecraft?",
			Message:  fmt.Sprintf("Stop %s now?", m.instance.Name),
			Warning:  "Progress not yet saved in-game may be lost. Quitting from the game menu is safer.",
			Confirm:  "Stop game",
			Cancel:   "Keep playing",
			Kind:     ConfirmDanger,
			FocusYes: m.stopFocusYes,
		}.View(m.width), m.width, m.height)
	}
	return base
}

// baseView renders the screen itself (overlays composite over it in View).
func (m *LaunchModel) baseView() string {
	panelW := min(60, max(24, m.width))

	// Header: the shared breadcrumb, with what's being launched flush right.
	where := "Launch"
	if m.status.Step == "Playing" {
		where = "Playing"
	}
	header := AppHeader(m.width, lipgloss.NewStyle().Foreground(Active.TextDim).
		Render("Minecraft "+m.instance.Version+" · "+loaderDisplay(m.instance)), m.instance.Name, where)

	// Steps under their real names: ✓ done, ● running (bright), ○ pending, ✗ failed.
	rows := make([]string, 0, len(m.steps))
	for _, step := range m.steps {
		label := stepLabel(step)
		switch step.status {
		case "done":
			rows = append(rows, lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDone)+" "+
				lipgloss.NewStyle().Foreground(Active.TextDim).Render(label))
		case "running":
			rows = append(rows, lipgloss.NewStyle().Foreground(Active.Primary).Render(GlyphDot)+" "+
				lipgloss.NewStyle().Bold(true).Foreground(Active.TextStrong).Render(label))
		case "error":
			rows = append(rows, lipgloss.NewStyle().Foreground(Active.Error).Render(GlyphFail+" "+label))
		default:
			rows = append(rows, lipgloss.NewStyle().Foreground(Active.TextMuted).Render(GlyphPending+" "+label))
		}
	}

	parts := []string{header, "", progressBar(m.percent, panelW), "", strings.Join(rows, "\n")}

	// One status line under the steps.
	msg := m.status.Message
	if m.playing() && !m.stopping {
		msg = "Minecraft is running. Esc goes home; the game keeps going."
	}
	if msg != "" && !(m.done && m.err != nil) {
		parts = append(parts, "", lipgloss.NewStyle().Foreground(Active.TextDim).Width(panelW).Render(msg))
	}

	// Completion / error / in-flight footer.
	var footer string
	switch {
	case m.done && m.err != nil:
		headline := lipgloss.NewStyle().Bold(true).Foreground(Active.Error).Render(GlyphFail + " Couldn't launch Minecraft")
		detail := lipgloss.NewStyle().Foreground(Active.TextDim).Width(panelW).Render(m.err.Error())
		parts = append(parts, "", headline, detail)
		footer = KeyHints(panelW, KeyHint{"r", "retry"}, KeyHint{"o", "play offline"}, KeyHint{"↵", "home"}, KeyHint{"?", "help"})
	case m.done:
		footer = lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDone + " Game closed. Returning home…")
	case m.stopping:
		footer = lipgloss.JoinVertical(lipgloss.Left,
			lipgloss.NewStyle().Foreground(Active.WarningStrong).Render("Stopping Minecraft… it may take a few seconds to save."),
			"", KeyHints(panelW, KeyHint{"esc", "home"}))
	case m.playing():
		hints := []KeyHint{{"esc", "home"}, {"s", "stop"}}
		if m.cfg != nil {
			v := launch.ParseLaunchLogVerbosity(m.cfg.LaunchLogVerbosity)
			hints = append(hints, KeyHint{"v", "logs: " + v.ShortLabel()})
		}
		footer = KeyHints(panelW, append(hints, KeyHint{"?", "help"})...)
	default:
		footer = KeyHints(panelW, KeyHint{"esc", "cancel"}, KeyHint{"?", "help"})
	}

	// Game log: only as many of the latest lines as fit the screen, so the
	// header and progress never scroll off (bubbletea drops the top rows of a
	// view taller than the terminal).
	if len(m.logs) > 0 {
		used := lipgloss.Height(lipgloss.JoinVertical(lipgloss.Left, parts...)) + 1 + lipgloss.Height(footer)
		room := m.height - used - 3 // blank + log title + rule
		if m.height <= 0 {
			room = len(m.logs)
		}
		if room > 0 {
			lines := m.logs[max(0, len(m.logs)-room):]
			logStyle := lipgloss.NewStyle().Foreground(Active.TextMuted)
			styled := make([]string, len(lines))
			for i, line := range lines {
				styled[i] = logStyle.Render(ansi.Truncate(line, panelW, "…"))
			}
			parts = append(parts, "", Zone("Game log", strings.Join(styled, "\n"), panelW, false))
		}
	}

	parts = append(parts, "", footer)
	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}
