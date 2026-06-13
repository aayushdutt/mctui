// Package ui settings provides the settings editor screen.
package ui

import (
	"os"
	"strings"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// settingsFocus is which control is active on the settings form.
type settingsFocus int

const (
	focusSettingsJavaPath settingsFocus = iota
	focusSettingsJVMArgs
	focusSettingsSnapshots
	focusSettingsMSAClientID
	focusSettingsSave
)

// settingsFocusOrder is the fixed Tab order of the form.
var settingsFocusOrder = []settingsFocus{
	focusSettingsJavaPath,
	focusSettingsJVMArgs,
	focusSettingsSnapshots,
	focusSettingsMSAClientID,
	focusSettingsSave,
}

// SettingsModel edits the user-facing subset of [config.Config]. It holds a
// working copy and emits [SettingsSaved] on submit; the app applies + persists.
type SettingsModel struct {
	width  int
	height int

	focus settingsFocus

	javaPath    textinput.Model
	jvmArgs     textinput.Model
	msaClientID textinput.Model
	snapshots   bool

	saveErr string
}

// NewSettingsModel seeds the form from cfg (read-only; nothing is mutated here).
func NewSettingsModel(cfg *config.Config) *SettingsModel {
	mk := func(value, placeholder string, width int) textinput.Model {
		ti := textinput.New()
		ti.SetValue(value)
		ti.Placeholder = placeholder
		ti.CharLimit = 512
		ti.Width = width
		ti.PromptStyle = lipgloss.NewStyle().Foreground(ColorZinc500)
		ti.TextStyle = lipgloss.NewStyle().Foreground(ColorText)
		return ti
	}

	m := &SettingsModel{
		focus:       focusSettingsJavaPath,
		javaPath:    mk(cfg.JavaPath, "Auto-detect (leave empty)", 48),
		jvmArgs:     mk(strings.Join(cfg.JVMArgs, " "), strings.Join(config.DefaultJVMArgs(), " "), 48),
		msaClientID: mk(cfg.MSAClientID, config.DefaultMSAClientID, 48),
		snapshots:   cfg.ShowSnapshots,
	}
	m.applyFocus(focusSettingsJavaPath)
	return m
}

// SetSize updates dimensions.
func (m *SettingsModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	w := min(60, max(24, width-6))
	m.javaPath.Width = w
	m.jvmArgs.Width = w
	m.msaClientID.Width = w
}

// Init implements tea.Model.
func (m *SettingsModel) Init() tea.Cmd {
	return textinput.Blink
}

func (m *SettingsModel) focusedInput() *textinput.Model {
	switch m.focus {
	case focusSettingsJavaPath:
		return &m.javaPath
	case focusSettingsJVMArgs:
		return &m.jvmArgs
	case focusSettingsMSAClientID:
		return &m.msaClientID
	}
	return nil
}

func (m *SettingsModel) applyFocus(f settingsFocus) {
	m.focus = f
	m.javaPath.Blur()
	m.jvmArgs.Blur()
	m.msaClientID.Blur()
	if in := m.focusedInput(); in != nil {
		in.Focus()
	}
}

func (m *SettingsModel) cycleFocus(delta int) {
	idx := 0
	for i, f := range settingsFocusOrder {
		if f == m.focus {
			idx = i
			break
		}
	}
	n := len(settingsFocusOrder)
	m.applyFocus(settingsFocusOrder[(idx+delta+n)%n])
}

// Update implements tea.Model.
func (m *SettingsModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		// Space toggles the snapshots checkbox when focused.
		if m.focus == focusSettingsSnapshots &&
			(msg.Type == tea.KeySpace || msg.String() == " " || msg.String() == "space") {
			m.snapshots = !m.snapshots
			return m, nil
		}
		switch msg.String() {
		case "esc":
			return m, func() tea.Msg { return NavigateToHome{} }
		case "tab", "down":
			m.cycleFocus(1)
			return m, textinput.Blink
		case "shift+tab", "up":
			m.cycleFocus(-1)
			return m, textinput.Blink
		case "enter":
			if m.focus == focusSettingsSnapshots {
				m.snapshots = !m.snapshots
				return m, nil
			}
			return m.submit()
		}
	}

	if in := m.focusedInput(); in != nil {
		var cmd tea.Cmd
		*in, cmd = in.Update(msg)
		return m, cmd
	}
	return m, nil
}

func (m *SettingsModel) submit() (*SettingsModel, tea.Cmd) {
	javaPath := strings.TrimSpace(m.javaPath.Value())
	if javaPath != "" {
		if _, err := os.Stat(javaPath); err != nil {
			m.saveErr = "Java path not found: " + javaPath
			m.applyFocus(focusSettingsJavaPath)
			return m, textinput.Blink
		}
	}
	m.saveErr = ""

	saved := SettingsSaved{
		JavaPath:      javaPath,
		JVMArgs:       strings.Fields(m.jvmArgs.Value()),
		ShowSnapshots: m.snapshots,
		MSAClientID:   strings.TrimSpace(m.msaClientID.Value()),
	}
	return m, func() tea.Msg { return saved }
}

// View implements tea.Model.
func (m *SettingsModel) View() string {
	header := lipgloss.NewStyle().Bold(true).Foreground(ColorText).Render("Settings")
	sub := lipgloss.NewStyle().Foreground(ColorSubtle).Render("Changes apply on save and persist to config.json")

	field := func(label, hint string, in textinput.Model, focused bool) string {
		border := ColorZinc700
		if focused {
			border = ColorSuccess
		}
		lbl := lipgloss.NewStyle().Foreground(ColorZinc500).Render(label)
		box := lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(border).
			Padding(0, 1).
			Render(in.View())
		hintLine := lipgloss.NewStyle().Foreground(ColorZinc600).Render(hint)
		return lipgloss.JoinVertical(lipgloss.Left, lbl, box, hintLine)
	}

	javaBlock := field("Java path", "Leave empty to auto-detect or download.", m.javaPath, m.focus == focusSettingsJavaPath)
	jvmBlock := field("JVM arguments", "Space-separated. Empty falls back to the default.", m.jvmArgs, m.focus == focusSettingsJVMArgs)
	msaBlock := field("Microsoft client ID", "Advanced. Empty uses the built-in default. Changing this may require signing in again.", m.msaClientID, m.focus == focusSettingsMSAClientID)

	// Snapshots checkbox row, styled to match the wizard's starter-mods row.
	cbFocused := m.focus == focusSettingsSnapshots
	mark := wizardCheckboxGlyph(m.snapshots, cbFocused)
	cbTitle := lipgloss.NewStyle().Foreground(ColorZinc200).Render("Show snapshots in the version list")
	cbSub := lipgloss.NewStyle().Foreground(ColorZinc500).Render("Includes pre-releases and weekly snapshots")
	cbLabel := lipgloss.JoinVertical(lipgloss.Left, cbTitle, cbSub)
	cbRow := lipgloss.JoinHorizontal(lipgloss.Top, mark, "  ", cbLabel)
	rowStyle := lipgloss.NewStyle().PaddingLeft(2)
	if cbFocused {
		rowStyle = lipgloss.NewStyle().
			Border(lipgloss.NormalBorder(), false, false, false, true).
			BorderForeground(ColorSuccess).
			Background(ColorZinc800).
			PaddingLeft(1).
			PaddingRight(1)
	}
	snapshotsBlock := rowStyle.Render(cbRow)

	saveBtn := lipgloss.NewStyle().MarginTop(1).Render(wizardFormButton("Save", m.focus == focusSettingsSave, true))

	errBlock := ""
	if m.saveErr != "" {
		errBlock = lipgloss.NewStyle().Foreground(ColorError).MarginTop(1).Render(m.saveErr)
	}

	help := lipgloss.NewStyle().
		Foreground(ColorZinc600).
		MarginTop(1).
		Render("[Tab]/[↑][↓] move · [Space] toggle · [Enter] save · [Esc] cancel")

	return lipgloss.JoinVertical(
		lipgloss.Left,
		header,
		sub,
		"",
		javaBlock,
		jvmBlock,
		snapshotsBlock,
		msaBlock,
		saveBtn,
		errBlock,
		help,
	)
}
