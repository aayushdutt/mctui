package ui

import (
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/lipgloss"
)

// underlineField renders a text input as a form field WITHOUT a box: a muted
// label over the input line, the line carrying a single bottom-border underline
// that turns accent (Primary) on focus. An optional faint hint sits beneath. This
// is the shared field renderer for the settings and edit-instance screens (twins)
// — the underline replaces the old rounded box whose whole border turned green on
// focus, so focus reads as one accent stroke consistent with the rest of the UI.
func underlineField(label, hint string, in textinput.Model, focused bool) string {
	lbl := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(label)

	under := Active.BorderSubtle
	if focused {
		under = Active.Primary
	}
	line := lipgloss.NewStyle().
		Border(lipgloss.NormalBorder(), false, false, true, false). // bottom edge only
		BorderForeground(under).
		Render(in.View())

	parts := []string{lbl, line}
	if hint != "" {
		parts = append(parts, lipgloss.NewStyle().Foreground(Active.TextFaint).Render(hint))
	}
	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}
