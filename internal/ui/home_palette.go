package ui

import (
	tea "github.com/charmbracelet/bubbletea"
)

// home_palette.go supplies the home screen's entries for the shared `:` command
// palette (kit_palette.go). Home's palette mixes high-frequency verbs with
// jump-to-instance rows; the overlay plumbing itself lives in [Palette].

// openPalette arms the palette overlay. Callers must already have ruled out the
// other overlays and filtering. Kept as a thin wrapper so call sites (and tests)
// read the same as before the extraction.
func (m *HomeModel) openPalette() {
	m.palette.Open()
}

// homePaletteItems builds the palette entries for the current state: the verb
// set (instance-scoped verbs no-op when the "+ New instance" row is selected)
// followed by one jump row per instance. Instance-scoped verbs read the live
// selection at Run time; the palette is modal, so the selection can't move while
// it is open.
func (m *HomeModel) homePaletteItems() []PaletteItem {
	playLabel := "Play"
	if sel := m.SelectedInstance(); sel != nil && sel.ID == m.playingID {
		playLabel = "View game"
	}
	items := []PaletteItem{
		{Label: playLabel, Key: "↵", Run: func() tea.Cmd { return m.launchSelected(false) }},
		{Label: "Play offline", Key: "o", Run: func() tea.Cmd { return m.launchSelected(true) }},
		{Label: "Mods", Key: "m", Run: m.paletteOpenMods},
		{Label: "Resource packs", Key: "p", Run: m.paletteOpenResourcePacks},
		{Label: "New instance", Key: "n", Run: func() tea.Cmd {
			return func() tea.Msg { return NavigateToNewInstance{} }
		}},
		{Label: "Edit instance", Key: "e", Run: m.paletteEditInstance},
		{Label: "Delete instance", Key: "d", Run: m.paletteDeleteInstance},
		{Label: "Open folder", Key: "f", Run: m.paletteOpenFolder},
		{Label: "Settings", Key: "s", Run: func() tea.Cmd {
			return func() tea.Msg { return NavigateToSettings{} }
		}},
		{Label: "Accounts", Key: "a", Run: func() tea.Cmd {
			return func() tea.Msg { return NavigateToAuth{} }
		}},
	}

	for i := range m.instances {
		idx := i
		name := m.instances[i].Name
		items = append(items, PaletteItem{
			Label: name,
			Kind:  PaletteJump,
			Run: func() tea.Cmd {
				m.jumpToInstance(idx)
				return nil
			},
		})
	}
	return items
}

// jumpToInstance moves the list cursor to the instance at the given full-slice
// index. It clears any applied filter first so the full-slice index and the
// visible-list index coincide (otherwise the jump lands on the wrong row).
func (m *HomeModel) jumpToInstance(idx int) {
	if idx < 0 || idx >= len(m.instances) {
		return
	}
	m.list.ResetFilter()
	m.list.Select(idx)
	m.resetDetailFocus()
}

func (m *HomeModel) paletteOpenMods() tea.Cmd {
	sel := m.SelectedInstance()
	if sel == nil {
		return nil
	}
	if ok, reason := CanBrowseMods(sel); !ok {
		return m.SetTransientBanner(reason)
	}
	return func() tea.Msg { return NavigateToMods{Instance: sel} }
}

func (m *HomeModel) paletteOpenResourcePacks() tea.Cmd {
	sel := m.SelectedInstance()
	if sel == nil {
		return nil
	}
	return func() tea.Msg { return NavigateToResourcePacks{Instance: sel} }
}

func (m *HomeModel) paletteEditInstance() tea.Cmd {
	sel := m.SelectedInstance()
	if sel == nil {
		return nil
	}
	return func() tea.Msg { return NavigateToEditInstance{Instance: sel} }
}

// paletteDeleteInstance arms the delete-confirm overlay for the current
// selection. The palette closes before Run executes, so confirmDelete becomes
// the sole active overlay.
func (m *HomeModel) paletteDeleteInstance() tea.Cmd {
	sel := m.SelectedInstance()
	if sel == nil {
		return nil
	}
	m.armDelete(sel)
	return nil
}

func (m *HomeModel) paletteOpenFolder() tea.Cmd {
	if sel := m.SelectedInstance(); sel != nil {
		openInstanceFolder(sel.Path)
	}
	return nil
}
