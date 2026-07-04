package ui

import (
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/sahilm/fuzzy"
)

// home_palette.go is the `:` command palette: a centered modal with a themed
// textinput and a fuzzy-filtered list of verbs + instances. It mirrors the
// delete-confirm / actions-popup overlay contract — an overlay flag on
// HomeModel, key handling intercepted BEFORE the home key switch, and a centered
// Render path in View. While open, keystrokes feed the textinput, NOT home.

// paletteKind distinguishes a verb entry from a jump-to-instance entry.
type paletteKind int

const (
	paletteVerb paletteKind = iota
	paletteInstance
)

// paletteEntry is one selectable palette row. For verbs, action names the verb;
// for instances, instIdx is the index into m.instances to jump to.
type paletteEntry struct {
	kind    paletteKind
	label   string
	action  string // verb id (mods/respacks/settings/new/edit/folder)
	instIdx int
}

// paletteVerbs is the fixed verb set, in default (empty-query) order.
var paletteVerbs = []paletteEntry{
	{kind: paletteVerb, label: "Mods", action: "mods"},
	{kind: paletteVerb, label: "Resource packs", action: "respacks"},
	{kind: paletteVerb, label: "Settings", action: "settings"},
	{kind: paletteVerb, label: "New instance", action: "new"},
	{kind: paletteVerb, label: "Edit instance", action: "edit"},
	{kind: paletteVerb, label: "Open folder", action: "folder"},
}

// openPalette arms the palette overlay. Callers must already have ruled out the
// other overlays and filtering.
func (m *HomeModel) openPalette() {
	ti := textinput.New()
	ti.Placeholder = "Jump to instance or run a command…"
	ti.CharLimit = 100
	ti.Width = 40
	ThemeTextInput(&ti)
	ti.Focus()
	m.paletteInput = ti
	m.palette = true
	m.paletteCursor = 0
	m.rebuildPaletteEntries()
}

// closePalette tears the overlay down.
func (m *HomeModel) closePalette() {
	m.palette = false
	m.paletteInput.Blur()
	m.paletteEntries = nil
	m.paletteCursor = 0
}

// paletteCandidates returns every selectable entry (all verbs + all instances)
// in a stable base order before fuzzy filtering.
func (m *HomeModel) paletteCandidates() []paletteEntry {
	out := make([]paletteEntry, 0, len(paletteVerbs)+len(m.instances))
	out = append(out, paletteVerbs...)
	for i, inst := range m.instances {
		out = append(out, paletteEntry{kind: paletteInstance, label: inst.Name, instIdx: i})
	}
	return out
}

// rebuildPaletteEntries refilters the candidates against the current query using
// sahilm/fuzzy, ranking by match score. An empty query shows everything in base
// order. The cursor is clamped to the new list.
func (m *HomeModel) rebuildPaletteEntries() {
	cands := m.paletteCandidates()
	query := strings.TrimSpace(m.paletteInput.Value())
	if query == "" {
		m.paletteEntries = cands
	} else {
		labels := make([]string, len(cands))
		for i, c := range cands {
			labels[i] = c.label
		}
		matches := fuzzy.Find(query, labels) // sorted by descending score
		filtered := make([]paletteEntry, 0, len(matches))
		for _, mt := range matches {
			filtered = append(filtered, cands[mt.Index])
		}
		m.paletteEntries = filtered
	}
	if m.paletteCursor >= len(m.paletteEntries) {
		m.paletteCursor = max(0, len(m.paletteEntries)-1)
	}
}

// updatePalette handles a key while the palette owns input. Runs BEFORE the home
// key switch. Returns (cmd, true) when the key was consumed by the overlay.
func (m *HomeModel) updatePalette(msg tea.KeyMsg) (tea.Cmd, bool) {
	switch msg.String() {
	case "esc":
		m.closePalette()
		return nil, true
	case "up", "ctrl+p":
		if m.paletteCursor > 0 {
			m.paletteCursor--
		}
		return nil, true
	case "down", "ctrl+n":
		if m.paletteCursor < len(m.paletteEntries)-1 {
			m.paletteCursor++
		}
		return nil, true
	case "enter":
		if m.paletteCursor < len(m.paletteEntries) {
			return m.activatePaletteEntry(m.paletteEntries[m.paletteCursor]), true
		}
		m.closePalette()
		return nil, true
	}

	// Everything else feeds the textinput, then refilter.
	var cmd tea.Cmd
	m.paletteInput, cmd = m.paletteInput.Update(msg)
	m.rebuildPaletteEntries()
	return cmd, true
}

// activatePaletteEntry runs the selected entry and closes the palette. Instances
// jump the list cursor; verbs emit the matching Navigate* message (respecting
// the mods-browser gate, which falls back to the transient banner).
func (m *HomeModel) activatePaletteEntry(e paletteEntry) tea.Cmd {
	m.closePalette()

	if e.kind == paletteInstance {
		if e.instIdx >= 0 && e.instIdx < len(m.instances) {
			// instIdx indexes the full instance slice; list.Select indexes the
			// VISIBLE list. Clear any applied filter first so they coincide,
			// otherwise the jump lands on the wrong (filtered) row.
			m.list.ResetFilter()
			m.list.Select(e.instIdx)
		}
		return nil
	}

	sel := m.SelectedInstance()
	switch e.action {
	case "mods":
		if sel == nil {
			return nil
		}
		if !instanceSupportsModsBrowser(sel) {
			m.SetTransientBanner("Mods browser isn't available for this instance.")
			return nil
		}
		return func() tea.Msg { return NavigateToMods{Instance: sel} }
	case "respacks":
		if sel == nil {
			return nil
		}
		return func() tea.Msg { return NavigateToResourcePacks{Instance: sel} }
	case "settings":
		return func() tea.Msg { return NavigateToSettings{} }
	case "new":
		return func() tea.Msg { return NavigateToNewInstance{} }
	case "edit":
		if sel == nil {
			return nil
		}
		return func() tea.Msg { return NavigateToEditInstance{Instance: sel} }
	case "folder":
		if sel != nil {
			openInstanceFolder(sel.Path)
		}
		return nil
	}
	return nil
}

// renderPalette builds the centered palette modal. Called from View only when
// the overlay is active.
func (m *HomeModel) renderPalette() string {
	accent := Active.Primary

	panelWidth := m.width - 8
	if panelWidth > 52 {
		panelWidth = 52
	}
	floor := min(34, max(20, m.width-8))
	if panelWidth < floor {
		panelWidth = floor
	}

	// Cap the visible list so the modal can't overrun a short terminal.
	maxRows := 8
	if avail := m.height - 8; avail < maxRows {
		maxRows = max(1, avail)
	}

	parts := []string{m.paletteInput.View(), ""}

	if len(m.paletteEntries) == 0 {
		parts = append(parts, lipgloss.NewStyle().Foreground(Active.TextMuted).Render("No matches"))
	} else {
		// Window the list around the cursor so the highlighted row stays visible.
		start := 0
		if m.paletteCursor >= maxRows {
			start = m.paletteCursor - maxRows + 1
		}
		end := min(len(m.paletteEntries), start+maxRows)
		for i := start; i < end; i++ {
			e := m.paletteEntries[i]
			tag := "  "
			tagColor := Active.TextMuted
			if e.kind == paletteInstance {
				tag = GlyphDot + " "
				tagColor = Active.Secondary
			}
			tagStr := lipgloss.NewStyle().Foreground(tagColor).Render(tag)
			lblStyle := lipgloss.NewStyle().Foreground(Active.Text)
			selected := i == m.paletteCursor
			if selected {
				lblStyle = lblStyle.Bold(true).Foreground(accent)
			}
			parts = append(parts, rowMarker(selected)+tagStr+lblStyle.Render(e.label))
		}
	}

	body := lipgloss.JoinVertical(lipgloss.Left, parts...)
	panel := Panel("Command palette", body, panelWidth, accent)

	hint := lipgloss.NewStyle().MarginTop(1).Render(KeyHints(
		panelWidth,
		KeyHint{"↑↓", "move"},
		KeyHint{"↵", "go"},
		KeyHint{"esc", "close"},
	))

	box := lipgloss.JoinVertical(lipgloss.Left, panel, hint)
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, box)
}
