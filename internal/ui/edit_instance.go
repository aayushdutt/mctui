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
	focusEditSave
)

// editInstanceFocusOrder is the fixed Tab order of the form.
var editInstanceFocusOrder = []editInstanceFocus{
	focusEditName,
	focusEditMemory,
	focusEditJavaPath,
	focusEditJVMArgs,
	focusEditSave,
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
}

// NewEditInstanceModel seeds the form from inst (read-only; nothing is mutated
// here — the app applies the edit when [InstanceEdited] lands).
func NewEditInstanceModel(inst *core.Instance) *EditInstanceModel {
	mk := func(value, placeholder string, width int) textinput.Model {
		ti := textinput.New()
		ti.SetValue(value)
		ti.Placeholder = placeholder
		ti.CharLimit = 512
		ti.Width = width
		ThemeTextInput(&ti)
		return ti
	}

	xmx, rest := splitXmx(inst.JVMArgs)

	m := &EditInstanceModel{
		inst:     inst,
		focus:    focusEditName,
		name:     mk(inst.Name, "Display name", 48),
		memory:   mk(xmx, "e.g. 4G or 2048M (leave empty for default)", 48),
		javaPath: mk(inst.JavaPath, "Auto-detect (leave empty)", 48),
		jvmArgs:  mk(strings.Join(rest, " "), "Extra space-separated flags (optional)", 48),
	}
	m.applyFocus(focusEditName)
	return m
}

// SetSize updates dimensions. The content area is already padded by the app
// shell, so no extra padding is applied here.
func (m *EditInstanceModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	w := min(60, max(24, width-6))
	m.name.Width = w
	m.memory.Width = w
	m.javaPath.Width = w
	m.jvmArgs.Width = w
}

// Init implements tea.Model.
func (m *EditInstanceModel) Init() tea.Cmd {
	return textinput.Blink
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
	header := ScreenHeader("Edit · "+m.inst.Name, "Per-instance overrides. Identity (version, loader) is fixed.")

	field := func(label, hint string, in textinput.Model, focused bool) string {
		border := Active.BorderSubtle
		if focused {
			border = Active.Success
		}
		lbl := lipgloss.NewStyle().Foreground(Active.TextDim).Render(label)
		box := lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(border).
			Padding(0, 1).
			Render(in.View())
		hintLine := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(hint)
		return lipgloss.JoinVertical(lipgloss.Left, lbl, box, hintLine)
	}

	nameBlock := field("Name", "Display name only. Does not move or rename the instance folder.", m.name, m.focus == focusEditName)
	memBlock := field("Memory (max heap)", "Sets -Xmx. e.g. 4G or 2048M. Empty leaves the default.", m.memory, m.focus == focusEditMemory)
	javaBlock := field("Java path", "Leave empty to auto-detect or download.", m.javaPath, m.focus == focusEditJavaPath)
	jvmBlock := field("JVM arguments", "Space-separated. The -Xmx flag is managed by the Memory field above.", m.jvmArgs, m.focus == focusEditJVMArgs)

	saveBtn := lipgloss.NewStyle().MarginTop(1).Render(wizardFormButton("Save", m.focus == focusEditSave, true))

	errBlock := ""
	if m.saveErr != "" {
		errBlock = lipgloss.NewStyle().Foreground(Active.Error).MarginTop(1).Render(m.saveErr)
	}

	help := lipgloss.NewStyle().MarginTop(1).Render(KeyHints(max(40, m.width-4),
		KeyHint{"tab", "move"},
		KeyHint{"enter", "save"},
		KeyHint{"esc", "cancel"},
	))

	return lipgloss.JoinVertical(
		lipgloss.Left,
		header,
		"",
		nameBlock,
		memBlock,
		javaBlock,
		jvmBlock,
		saveBtn,
		errBlock,
		help,
	)
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
