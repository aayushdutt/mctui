package ui

import (
	"github.com/charmbracelet/lipgloss"
)

// kit_dialog.go is the shared confirm-dialog kit. Destructive (and neutral)
// confirmations across screens render through one [ConfirmDialog] so they look
// and behave identically: a centered modal card, a focused Yes/No row, and a
// fixed key bar. Screens own the state and key handling; the toggle keys they
// handle should match [ConfirmKeyToggles].

// ConfirmKind selects the accent/tone of a [ConfirmDialog].
type ConfirmKind int

const (
	// ConfirmNeutral uses the primary accent (ordinary confirmation).
	ConfirmNeutral ConfirmKind = iota
	// ConfirmDanger uses the error accent (destructive confirmation).
	ConfirmDanger
)

// ConfirmDialog is a centered modal yes/no confirmation. Build it fresh each
// frame from screen state and call [ConfirmDialog.Render].
type ConfirmDialog struct {
	Title    string // panel title, e.g. "Delete instance?"
	Message  string // body copy, e.g. `Delete "Survival 1.21"?` (may be multi-line)
	Warning  string // optional caution line, rendered with GlyphWarn in the accent color
	Confirm  string // confirm-option label, e.g. "Delete"
	Cancel   string // cancel-option label, e.g. "Cancel"
	Kind     ConfirmKind
	FocusYes bool // true → confirm option highlighted; false → cancel highlighted
}

// ConfirmKeyToggles reports whether key should flip a dialog's FocusYes: the
// arrows, hjkl-style up/down, and tab. Screens call this to keep toggle keys
// identical across confirmations.
func ConfirmKeyToggles(key string) bool {
	switch key {
	case "left", "right", "up", "down", "h", "j", "k", "l", "tab":
		return true
	}
	return false
}

// Render returns the dialog as a centered modal filling w x h on a blank
// background. (lipgloss v1 can't composite over the underlying screen, so a
// centered modal on blank is the consistent choice.)
func (d ConfirmDialog) Render(w, h int) string {
	accent := Active.Primary
	if d.Kind == ConfirmDanger {
		accent = Active.Error
	}

	// Clamp to [floor, 56] but never exceed the terminal: on very narrow
	// terminals the floor itself shrinks (min(34, max(20, w-8))) so the box
	// always fits instead of overflowing.
	panelWidth := w - 8
	if panelWidth > 56 {
		panelWidth = 56
	}
	floor := min(34, max(20, w-8))
	if panelWidth < floor {
		panelWidth = floor
	}

	msg := lipgloss.NewStyle().Foreground(Active.Text).Render(d.Message)

	parts := []string{msg, ""}
	if d.Warning != "" {
		warn := lipgloss.NewStyle().Bold(true).Foreground(accent).
			Render(GlyphWarn + " " + d.Warning)
		parts = append(parts, warn, "")
	}

	// Yes/No row: focused option carries the pointer + bold accent; the other
	// is muted with no marker.
	confirmSt := lipgloss.NewStyle().Foreground(Active.TextMuted)
	cancelSt := lipgloss.NewStyle().Foreground(Active.TextMuted)
	confirmLbl := d.Confirm
	cancelLbl := d.Cancel
	if d.FocusYes {
		confirmSt = lipgloss.NewStyle().Bold(true).Foreground(accent)
		confirmLbl = GlyphPointer + " " + confirmLbl
	} else {
		cancelSt = lipgloss.NewStyle().Bold(true).Foreground(accent)
		cancelLbl = GlyphPointer + " " + cancelLbl
	}
	row := lipgloss.JoinHorizontal(lipgloss.Left,
		confirmSt.Render(confirmLbl),
		"        ",
		cancelSt.Render(cancelLbl),
	)
	parts = append(parts, row)

	body := lipgloss.JoinVertical(lipgloss.Left, parts...)
	panel := Panel(d.Title, body, panelWidth, accent)

	hint := lipgloss.NewStyle().MarginTop(1).Render(KeyHints(
		panelWidth,
		KeyHint{"↑↓/tab", "choose"},
		KeyHint{"↵", "confirm"},
		KeyHint{"esc", "cancel"},
	))

	box := lipgloss.JoinVertical(lipgloss.Left, panel, hint)
	return lipgloss.Place(w, h, lipgloss.Center, lipgloss.Center, box)
}
