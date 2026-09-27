// Package ui edit_instance provides the per-instance Edit screen. It edits the
// mutable, per-instance fields of a [core.Instance] — display Name, max heap
// (-Xmx), Java path override, and extra JVM arguments — leaving the instance's
// identity (ID, Path, Version, Loader) untouched. It mirrors the global
// SettingsModel form: textinput-based, focus cycling, esc→home, enter→save.
package ui

import (
	"os"
	"strings"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// editInstanceFocus is which control is active on the edit form.
type editInstanceFocus int

const (
	focusEditName editInstanceFocus = iota
	focusEditMemory
	focusEditJavaPath
	focusEditJVMArgs
)

// editInstanceFocusOrder is the fixed Tab order of the form.
var editInstanceFocusOrder = []editInstanceFocus{
	focusEditName,
	focusEditMemory,
	focusEditJavaPath,
	focusEditJVMArgs,
}

// EditInstanceModel edits the per-instance override fields of a [core.Instance].
// It holds a working copy of the editable values and emits [InstanceEdited] on
// submit; the app persists via the InstanceManager. The memory field surfaces the
// -Xmx token out of JVMArgs so it can be edited on its own; the JVM args field
// then shows every OTHER token. On save the two are folded back together.
type EditInstanceModel struct {
	width  int
	height int

	inst  *core.Instance
	focus editInstanceFocus

	name     textinput.Model
	memory   textinput.Model
	javaPath textinput.Model
	jvmArgs  textinput.Model

	saveErr string

	// initial is the form as it opened; discardArmed is set by a first esc
	// with unsaved changes (see SettingsModel).
	initial      [4]string
	discardArmed bool

	palette CommandPalette

	// help is the `?` read-only key reference overlay (kit_help.go), dismissed by
	// ? / esc / q.
	help bool
}

// helpGroups returns the edit-instance screen's bindings grouped for the `?`
// overlay, hand-built to match exactly what Update handles (field cycling, save)
// and the footer hints.
func (m *EditInstanceModel) helpGroups() []HelpGroup {
	return []HelpGroup{
		{Title: "Navigate", Entries: []HelpEntry{
			{"↑↓ / tab", "move between fields"},
		}},
		{Title: "Act", Entries: []HelpEntry{
			{"enter", "save (from any field)"},
		}},
		{Title: "General", Entries: []HelpEntry{
			{":", "command palette"},
			{"?", "help"},
			{"esc", "back home"},
		}},
	}
}

// NewEditInstanceModel seeds the form from inst (read-only; nothing is mutated
// here — the app applies the edit when [InstanceEdited] lands).
func NewEditInstanceModel(inst *core.Instance) *EditInstanceModel {
	xmx, rest := splitXmx(inst.JVMArgs)

	m := &EditInstanceModel{
		inst:     inst,
		focus:    focusEditName,
		name:     formInput(inst.Name, "Display name"),
		memory:   formInput(xmx, "Default"),
		javaPath: formInput(inst.JavaPath, "Automatic"),
		jvmArgs:  formInput(strings.Join(rest, " "), "None"),
	}
	m.applyFocus(focusEditName)
	m.initial = m.values()
	m.palette = NewCommandPalette("Edit instance · commands", "Run a command…", m.editPaletteItems)
	return m
}

// editPaletteItems supplies the edit-instance screen's `:` command-palette verbs.
func (m *EditInstanceModel) editPaletteItems() []PaletteItem {
	return []PaletteItem{
		{Label: "Save", Run: func() tea.Cmd {
			_, cmd := m.submit()
			return cmd
		}},
		{Label: "Back to home", Run: func() tea.Cmd {
			return func() tea.Msg { return NavigateToHome{} }
		}},
	}
}

// SetSize updates dimensions. The content area is already padded by the app
// shell, so no extra padding is applied here.
func (m *EditInstanceModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	w := formValueWidth(width)
	m.name.Width = w
	m.memory.Width = w
	m.javaPath.Width = w
	m.jvmArgs.Width = w
}

// Init implements tea.Model.
func (m *EditInstanceModel) Init() tea.Cmd {
	return textinput.Blink
}

func (m *EditInstanceModel) values() [4]string {
	return [4]string{m.name.Value(), m.memory.Value(), m.javaPath.Value(), m.jvmArgs.Value()}
}

// moveFocus steps focus up/down the fields, stopping at the first and last.
func (m *EditInstanceModel) moveFocus(dir int) {
	for i, f := range editInstanceFocusOrder {
		if f == m.focus {
			m.applyFocus(editInstanceFocusOrder[clampStep(i, dir, len(editInstanceFocusOrder))])
			return
		}
	}
}

func (m *EditInstanceModel) focusedInput() *textinput.Model {
	switch m.focus {
	case focusEditName:
		return &m.name
	case focusEditMemory:
		return &m.memory
	case focusEditJavaPath:
		return &m.javaPath
	case focusEditJVMArgs:
		return &m.jvmArgs
	}
	return nil
}

func (m *EditInstanceModel) applyFocus(f editInstanceFocus) {
	m.focus = f
	m.name.Blur()
	m.memory.Blur()
	m.javaPath.Blur()
	m.jvmArgs.Blur()
	if in := m.focusedInput(); in != nil {
		in.Focus()
	}
}

func (m *EditInstanceModel) cycleFocus(delta int) {
	idx := 0
	for i, f := range editInstanceFocusOrder {
		if f == m.focus {
			idx = i
			break
		}
	}
	n := len(editInstanceFocusOrder)
	m.applyFocus(editInstanceFocusOrder[(idx+delta+n)%n])
}

// Update implements tea.Model.
func (m *EditInstanceModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
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
		switch msg.String() {
		case ":":
			// Open the palette only when no text input is focused (Save row), so ":"
			// still types a literal colon into a focused field.
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
			if m.values() != m.initial && !m.discardArmed {
				// Don't throw typed changes away on one keypress.
				m.discardArmed = true
				return m, nil
			}
			return m, func() tea.Msg { return NavigateToHome{} }
		case "tab":
			m.cycleFocus(1)
			return m, textinput.Blink
		case "shift+tab":
			m.cycleFocus(-1)
			return m, textinput.Blink
		case "down":
			m.moveFocus(1)
			return m, textinput.Blink
		case "up":
			m.moveFocus(-1)
			return m, textinput.Blink
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

func (m *EditInstanceModel) submit() (*EditInstanceModel, tea.Cmd) {
	name := strings.TrimSpace(m.name.Value())
	if name == "" {
		m.saveErr = "Name cannot be empty."
		m.applyFocus(focusEditName)
		return m, textinput.Blink
	}

	mem := strings.TrimSpace(m.memory.Value())
	if mem != "" && !validMemory(mem) {
		m.saveErr = "Memory must be a number with an optional K/M/G suffix (e.g. 4G, 2048M)."
		m.applyFocus(focusEditMemory)
		return m, textinput.Blink
	}

	javaPath := strings.TrimSpace(m.javaPath.Value())
	if javaPath != "" {
		if _, err := os.Stat(javaPath); err != nil {
			// Warn but still allow the save — the path may become valid later.
			m.saveErr = "Warning: Java path not found: " + javaPath
		} else {
			m.saveErr = ""
		}
	} else {
		m.saveErr = ""
	}

	// Apply edits to the working instance. Identity fields (ID, Path, Version,
	// Loader) are deliberately left untouched.
	m.inst.Name = name
	m.inst.JavaPath = javaPath
	m.inst.JVMArgs = foldXmx(mem, strings.Fields(m.jvmArgs.Value()))

	inst := m.inst
	return m, func() tea.Msg { return InstanceEdited{Instance: inst} }
}

// View implements tea.Model.
func (m *EditInstanceModel) View() string {
	base := m.baseView()
	switch {
	case m.help:
		return Overlay(base, HelpCard("Edit instance", m.helpGroups(), m.width, m.height), m.width, m.height)
	case m.palette.Active:
		return Overlay(base, m.palette.View(m.width, m.height), m.width, m.height)
	}
	return base
}

// baseView renders the screen itself (overlays composite over it in View).
func (m *EditInstanceModel) baseView() string {
	header := AppHeader(m.width, lipgloss.NewStyle().Foreground(Active.TextDim).
		Render("Minecraft "+m.inst.Version+" · "+loaderDisplay(m.inst)), m.inst.Name, "Edit")
	f := m.focus

	rows := []string{
		header, "",
		formSection("Instance"),
		formRow("Name", m.name.View(), "Display name only; the folder isn't renamed.", f == focusEditName, m.width),
		"",
		formSection("Java"),
		formRow("Memory", m.memory.View(), "Max heap, e.g. 4G or 2048M. Empty uses the default.", f == focusEditMemory, m.width),
		formRow("Java path", m.javaPath.View(), "Empty finds or downloads the right Java for this version.", f == focusEditJavaPath, m.width),
		formRow("JVM arguments", m.jvmArgs.View(), "Extra space-separated flags. Memory is set above.", f == focusEditJVMArgs, m.width),
	}
	if m.saveErr != "" {
		rows = append(rows, "", lipgloss.NewStyle().Foreground(Active.Error).Render(GlyphFail+" "+m.saveErr))
	}
	if m.discardArmed {
		rows = append(rows, "", formDiscardHint())
	}
	// Every field is a text input, so ? and : type into it — not offered here.
	rows = append(rows, "", KeyHints(m.width,
		KeyHint{"↵", "save"}, KeyHint{"↑↓", "move"}, KeyHint{"esc", "cancel"}))
	return lipgloss.JoinVertical(lipgloss.Left, rows...)
}

// splitXmx surfaces the -Xmx value (e.g. "4G") out of a JVM-args slice and
// returns it alongside the remaining args (in order, -Xmx removed). If multiple
// -Xmx tokens are present the last one wins, matching the JVM. The returned value
// is "" when no -Xmx flag is set.
func splitXmx(args []string) (xmx string, rest []string) {
	rest = make([]string, 0, len(args))
	for _, a := range args {
		if v, ok := strings.CutPrefix(a, "-Xmx"); ok {
			xmx = v
			continue
		}
		rest = append(rest, a)
	}
	return xmx, rest
}

// foldXmx recombines a memory value with the remaining JVM args. A non-empty mem
// is prepended as a single "-Xmx<mem>" token; any stray -Xmx tokens already in
// rest are dropped first so the result has at most one. An empty mem means "no
// -Xmx override", so none is added.
func foldXmx(mem string, rest []string) []string {
	out := make([]string, 0, len(rest)+1)
	if mem != "" {
		out = append(out, "-Xmx"+mem)
	}
	for _, a := range rest {
		if strings.HasPrefix(a, "-Xmx") {
			continue
		}
		out = append(out, a)
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

// validMemory reports whether s is a JVM heap size: one or more digits with an
// optional single K/M/G suffix (case-insensitive), e.g. "4G", "2048M", "512".
func validMemory(s string) bool {
	if s == "" {
		return false
	}
	digits := s
	switch s[len(s)-1] {
	case 'k', 'K', 'm', 'M', 'g', 'G':
		digits = s[:len(s)-1]
	}
	if digits == "" {
		return false
	}
	for _, r := range digits {
		if r < '0' || r > '9' {
			return false
		}
	}
	return true
}
