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
	focusSettingsTheme settingsFocus = iota
	focusSettingsSnapshots
	focusSettingsJavaPath
	focusSettingsJVMArgs
	focusSettingsMSAClientID
)

// settingsFocusOrder is the fixed Tab / ↑↓ order of the form, top to bottom.
var settingsFocusOrder = []settingsFocus{
	focusSettingsTheme,
	focusSettingsSnapshots,
	focusSettingsJavaPath,
	focusSettingsJVMArgs,
	focusSettingsMSAClientID,
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

	themeNames []string // registered theme names, in order
	themeIdx   int      // index of the previewed/selected theme
	origTheme  string   // theme active when the form opened (restore on cancel)

	saveErr string

	// initial is the form as it opened, to tell whether esc would lose edits;
	// discardArmed is set by a first esc with unsaved changes.
	initial      settingsSnapshot
	discardArmed bool

	palette CommandPalette

	// help is the `?` read-only key reference overlay (kit_help.go), dismissed by
	// ? / esc / q.
	help bool
}

// settingsSnapshot is the form's editable state, for change detection.
type settingsSnapshot struct {
	javaPath, jvmArgs, msaClientID string
	snapshots                      bool
	themeIdx                       int
}

func (m *SettingsModel) snapshot() settingsSnapshot {
	return settingsSnapshot{m.javaPath.Value(), m.jvmArgs.Value(), m.msaClientID.Value(), m.snapshots, m.themeIdx}
}

// dirty reports whether the form has changes that esc would throw away.
func (m *SettingsModel) dirty() bool {
	return m.snapshot() != m.initial
}

// moveFocus steps focus up/down the rows, stopping at the first and last.
func (m *SettingsModel) moveFocus(dir int) {
	for i, f := range settingsFocusOrder {
		if f == m.focus {
			m.applyFocus(settingsFocusOrder[clampStep(i, dir, len(settingsFocusOrder))])
			return
		}
	}
}

// helpGroups returns the settings screen's bindings grouped for the `?` overlay,
// hand-built to match exactly what Update handles (field cycling, theme picker,
// snapshots toggle, save) and the footer hints.
func (m *SettingsModel) helpGroups() []HelpGroup {
	return []HelpGroup{
		{Title: "Navigate", Entries: []HelpEntry{
			{"↑↓ / tab", "move between fields"},
			{"← →", "change theme"},
			{"space", "toggle snapshots"},
		}},
		{Title: "Act", Entries: []HelpEntry{
			{"enter", "save (from any field)"},
		}},
		{Title: "General", Entries: []HelpEntry{
			{":", "command palette"},
			{"?", "help"},
			{"esc", "cancel · back home"},
		}},
	}
}

// NewSettingsModel seeds the form from cfg (read-only; nothing is mutated here).
// It opens on the Theme row — not a text field — so ? and : work right away.
func NewSettingsModel(cfg *config.Config) *SettingsModel {
	themeNames := ThemeNames()
	themeIdx := 0
	for i, n := range themeNames {
		if n == cfg.Theme {
			themeIdx = i
			break
		}
	}

	m := &SettingsModel{
		focus:       focusSettingsTheme,
		javaPath:    formInput(cfg.JavaPath, "Automatic"),
		jvmArgs:     formInput(strings.Join(cfg.JVMArgs, " "), strings.Join(config.DefaultJVMArgs(), " ")),
		msaClientID: formInput(customClientID(cfg.MSAClientID), "Built-in"),
		snapshots:   cfg.ShowSnapshots,
		themeNames:  themeNames,
		themeIdx:    themeIdx,
		origTheme:   ActiveName(),
	}
	m.applyFocus(focusSettingsTheme)
	m.initial = m.snapshot()
	m.palette = NewCommandPalette("Settings · commands", "Run a command…", m.settingsPaletteItems)
	return m
}

// customClientID is the client ID to show in the form: empty (the "Built-in"
// placeholder) when it's the default, so the form doesn't show a GUID nobody
// set. Saving empty keeps using the default.
func customClientID(id string) string {
	if id == config.DefaultMSAClientID {
		return ""
	}
	return id
}

// settingsPaletteItems supplies the settings screen's `:` command-palette verbs.
func (m *SettingsModel) settingsPaletteItems() []PaletteItem {
	return []PaletteItem{
		{Label: "Save", Run: func() tea.Cmd {
			_, cmd := m.submit()
			return cmd
		}},
		{Label: "Back to home", Run: func() tea.Cmd {
			// Mirror esc: drop any previewed-but-unsaved theme before leaving.
			Apply(m.origTheme)
			return func() tea.Msg { return NavigateToHome{} }
		}},
	}
}

// SetSize updates dimensions.
func (m *SettingsModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	w := formValueWidth(width)
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

// cycleTheme moves the theme selection by delta (wrapping) and applies the new
// theme immediately so the foreground form previews it live.
func (m *SettingsModel) cycleTheme(delta int) {
	n := len(m.themeNames)
	if n == 0 {
		return
	}
	m.themeIdx = (m.themeIdx + delta + n) % n
	Apply(m.themeNames[m.themeIdx])
}

// Update implements tea.Model.
func (m *SettingsModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		if m.palette.Active {
			return m, m.palette.Update(msg)
		}
		if msg.String() != "esc" {
			m.discardArmed = false
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
		// Space toggles the snapshots checkbox when focused.
		if m.focus == focusSettingsSnapshots &&
			(msg.Type == tea.KeySpace || msg.String() == " " || msg.String() == "space") {
			m.snapshots = !m.snapshots
			return m, nil
		}
		// Left/right cycles the theme selector with live preview when focused.
		// Handled here so it never falls through to the textinput handling.
		if m.focus == focusSettingsTheme {
			switch msg.String() {
			case "left", "h":
				m.cycleTheme(-1)
				return m, nil
			case "right", "l":
				m.cycleTheme(1)
				return m, nil
			}
		}
		switch msg.String() {
		case ":":
			// Open the command palette only when no text input is focused (otherwise
			// ":" must type a literal colon into the field).
			if m.focusedInput() == nil {
				m.palette.Open()
				return m, textinput.Blink
			}
		case "?":
			// Open the read-only key reference — gated exactly like ":" so "?" stays
			// typeable in a focused field.
			if m.focusedInput() == nil {
				m.help = true
				return m, nil
			}
		case "esc":
			if m.dirty() && !m.discardArmed {
				// Don't throw typed changes away on one keypress.
				m.discardArmed = true
				return m, nil
			}
			// Restore a previewed-but-unsaved theme so it doesn't persist visually.
			Apply(m.origTheme)
			return m, func() tea.Msg { return NavigateToHome{} }
		case "tab":
			m.cycleFocus(1)
			return m, textinput.Blink
		case "shift+tab":
			m.cycleFocus(-1)
			return m, textinput.Blink
		case "down", "up":
			m.moveFocus(map[string]int{"down": 1, "up": -1}[msg.String()])
			return m, textinput.Blink
		case "j", "k":
			// j/k move on rows that aren't text fields (there they type).
			if m.focusedInput() == nil {
				m.moveFocus(map[string]int{"j": 1, "k": -1}[msg.String()])
				return m, textinput.Blink
			}
		case "enter":
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
		Theme:         m.themeNames[m.themeIdx],
	}
	return m, func() tea.Msg { return saved }
}

// View implements tea.Model.
func (m *SettingsModel) View() string {
	base := m.baseView()
	switch {
	case m.help:
		return Overlay(base, HelpCard("Settings", m.helpGroups(), m.width, m.height), m.width, m.height)
	case m.palette.Active:
		return Overlay(base, m.palette.View(m.width, m.height), m.width, m.height)
	}
	return base
}

// baseView renders the screen itself (overlays composite over it in View).
func (m *SettingsModel) baseView() string {
	header := AppHeader(m.width, "", "Settings")
	f := m.focus

	themeName := ""
	if len(m.themeNames) > 0 {
		themeName = m.themeNames[m.themeIdx]
	}
	if themeName != "" {
		themeName = strings.ToUpper(themeName[:1]) + themeName[1:]
	}
	arrow := lipgloss.NewStyle().Foreground(Active.TextMuted)
	if f == focusSettingsTheme {
		arrow = arrow.Foreground(Active.Primary)
	}
	theme := arrow.Render("‹ ") + lipgloss.NewStyle().Foreground(Active.Text).Render(themeName) + arrow.Render(" ›")

	rows := []string{
		header, "",
		formSection("General"),
		formRow("Theme", theme, "Live preview. ← → to change.", f == focusSettingsTheme, m.width),
		formRow("Show snapshots", formToggle(m.snapshots), "Include snapshots and pre-releases in the version list. Space to toggle.", f == focusSettingsSnapshots, m.width),
		"",
		formSection("Java"),
		formRow("Java path", m.javaPath.View(), "Empty finds or downloads the right Java for each version.", f == focusSettingsJavaPath, m.width),
		formRow("JVM arguments", m.jvmArgs.View(), "Space-separated. Empty uses the defaults.", f == focusSettingsJVMArgs, m.width),
		"",
		formSection("Advanced"),
		formRow("Microsoft client ID", m.msaClientID.View(), "Empty uses the built-in ID. Changing it may require signing in again.", f == focusSettingsMSAClientID, m.width),
	}
	if m.saveErr != "" {
		rows = append(rows, "", lipgloss.NewStyle().Foreground(Active.Error).Render(GlyphFail+" "+m.saveErr))
	}
	if m.discardArmed {
		rows = append(rows, "", formDiscardHint())
	}

	// Footer: only keys that work right now. While a text field is focused, ?
	// and : type into it, so they aren't offered.
	hints := []KeyHint{{"↵", "save"}, {"↑↓", "move"}}
	switch f {
	case focusSettingsTheme:
		hints = append(hints, KeyHint{"←→", "theme"})
	case focusSettingsSnapshots:
		hints = append(hints, KeyHint{"space", "toggle"})
	}
	hints = append(hints, KeyHint{"esc", "cancel"})
	if m.focusedInput() == nil {
		hints = append(hints, KeyHint{"?", "help"})
	}
	rows = append(rows, "", KeyHints(m.width, hints...))
	return lipgloss.JoinVertical(lipgloss.Left, rows...)
}
