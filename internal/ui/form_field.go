package ui

import (
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/lipgloss"
)

// formLabelW is the label column width of the settings / edit-instance forms.
const formLabelW = 22

// formInput builds a text input for a form row: no prompt (the label column
// says what it is), themed, CharLimit 512.
func formInput(value, placeholder string) textinput.Model {
	ti := textinput.New()
	ti.Prompt = ""
	ti.SetValue(value)
	ti.Placeholder = placeholder
	ti.CharLimit = 512
	ti.Width = 48
	ThemeTextInput(&ti)
	return ti
}

// formValueWidth is the width left for a form row's value at screen width w.
func formValueWidth(w int) int {
	return max(12, min(60, w-2-formLabelW-1))
}

// formRow renders one form field as a label/value row:
//
//	› Java path             /usr/bin/java
//	                        Empty = find or download the right Java.
//
// The focused row gets the › pointer and a bright label, and its hint (the
// field's help) shows in a dim line under the value; other rows show just a dim
// label and the value. width is the screen width, for wrapping the hint.
func formRow(label, value, hint string, focused bool, width int) string {
	lbl := lipgloss.NewStyle().Foreground(Active.TextDim).Width(formLabelW)
	if focused {
		lbl = lbl.Bold(true).Foreground(Active.TextStrong)
	}
	row := rowMarker(focused) + lbl.Render(label) + value
	if !focused || hint == "" {
		return row
	}
	indent := lipgloss.NewStyle().PaddingLeft(2 + formLabelW)
	h := lipgloss.NewStyle().Foreground(Active.TextMuted).Width(formValueWidth(width)).Render(hint)
	return lipgloss.JoinVertical(lipgloss.Left, row, indent.Render(h))
}

// formSection renders a group heading inside a form ("Java"), aligned with the
// row labels.
func formSection(title string) string {
	return "  " + lipgloss.NewStyle().Bold(true).Foreground(Active.TextMuted).Render(title)
}

// formToggle renders an on/off value: "On" in the success color, "Off" dim.
func formToggle(on bool) string {
	if on {
		return lipgloss.NewStyle().Foreground(Active.Success).Render("On")
	}
	return lipgloss.NewStyle().Foreground(Active.TextDim).Render("Off")
}

// formDiscardHint is shown while a first esc on a form with unsaved changes is
// waiting for a second.
func formDiscardHint() string {
	return lipgloss.NewStyle().Foreground(Active.Warning).Render(GlyphWarn + " Unsaved changes — esc again to discard, ↵ to save.")
}

// clampStep moves index i by dir within [0, n), stopping at the ends.
func clampStep(i, dir, n int) int {
	return min(n-1, max(0, i+dir))
}
