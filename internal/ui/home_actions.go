package ui

import (
	"github.com/aayushdutt/mctui/internal/core"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// home_actions.go is the `x` actions popup: a small centered modal listing
// per-instance housekeeping actions (Edit / Open folder / Delete). It mirrors
// the delete-confirm overlay pattern — an overlay flag on HomeModel, key
// handling that intercepts BEFORE the normal key switch, and a centered Render
// path in View. This is the "long-tail" surface so the main key bar stays lean.

// homeAction is one entry in the actions popup: a hotkey letter, a label, and a
// danger flag for destructive entries (rendered in the error accent).
type homeAction struct {
	key    string
	label  string
	danger bool
}

// homeActions is the fixed action set, in display order. The hotkeys here must
// not collide with the popup's own controls (esc).
var homeActions = []homeAction{
	{key: "e", label: "Edit"},
	{key: "f", label: "Open folder"},
	{key: "d", label: "Delete", danger: true},
}

// openActionsPopup arms the actions overlay for inst. Mirrors the delete-confirm
// arming: callers must already have ruled out the other overlays and filtering.
func (m *HomeModel) openActionsPopup(inst *core.Instance) {
	m.actionsPopup = true
	m.actionsTarget = inst
	m.actionsCursor = 0
}

// closeActionsPopup tears the overlay down.
func (m *HomeModel) closeActionsPopup() {
	m.actionsPopup = false
	m.actionsTarget = nil
	m.actionsCursor = 0
}

// updateActionsPopup handles a key while the actions popup owns input. It runs
// BEFORE the home key switch (wired in Update), so home shortcuts can't leak
// through. Returns (cmd, true) when the key was consumed by the overlay.
func (m *HomeModel) updateActionsPopup(msg tea.KeyMsg) (tea.Cmd, bool) {
	switch msg.String() {
	case "esc", "q":
		m.closeActionsPopup()
		return nil, true
	case "up", "k":
		if m.actionsCursor > 0 {
			m.actionsCursor--
		}
		return nil, true
	case "down", "j":
		if m.actionsCursor < len(homeActions)-1 {
			m.actionsCursor++
		}
		return nil, true
	case "enter":
		return m.runAction(homeActions[m.actionsCursor]), true
	}

	// Hotkey letters fire their action directly.
	for _, a := range homeActions {
		if msg.String() == a.key {
			return m.runAction(a), true
		}
	}
	return nil, true
}

// runAction performs the chosen action and closes the popup. Delete re-enters
// the EXISTING delete-confirm flow rather than duplicating delete logic.
func (m *HomeModel) runAction(a homeAction) tea.Cmd {
	inst := m.actionsTarget
	m.closeActionsPopup()
	if inst == nil {
		return nil
	}
	switch a.key {
	case "e":
		return func() tea.Msg { return NavigateToEditInstance{Instance: inst} }
	case "f":
		openInstanceFolder(inst.Path)
		return nil
	case "d":
		// Hand off to the delete-confirm overlay (Cancel focused).
		m.armDelete(inst)
		return nil
	}
	return nil
}

// renderActionsPopup builds the actions card (composited over home by View).
// Called from View only when the overlay is active.
func (m *HomeModel) renderActionsPopup() string {
	cw := cardWidth(m.width, 44)
	inner := cw - 4

	title := "Actions"
	if m.actionsTarget != nil {
		title = m.actionsTarget.Name
	}

	rows := make([]string, 0, len(homeActions)+2)
	for i, a := range homeActions {
		selected := i == m.actionsCursor
		lbl := lipgloss.NewStyle().Foreground(Active.Text)
		if a.danger {
			lbl = lbl.Foreground(Active.Error)
		}
		if selected {
			lbl = lbl.Bold(true)
			if !a.danger {
				lbl = lbl.Foreground(Active.TextStrong)
			}
		}
		key := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(a.key) + " "
		rows = append(rows, StatusBar(rowMarker(selected)+lbl.Render(a.label), key, inner))
	}
	rows = append(rows, "", KeyHints(inner, KeyHint{"↑↓", "move"}, KeyHint{"↵", "run"}, KeyHint{"esc", "close"}))
	return Card(title, lipgloss.JoinVertical(lipgloss.Left, rows...), cw, Active.Primary)
}
