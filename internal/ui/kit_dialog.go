package ui

import (
	"github.com/charmbracelet/lipgloss"
)

// kit_dialog.go is the shared confirm-dialog kit. Destructive (and neutral)
// confirmations across screens render through one [ConfirmDialog] so they look
// and behave identically: a floating card, a focused Yes/No row, and a fixed
// key bar. Screens own the state and key handling; the toggle keys they
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

// ConfirmCancels reports whether key dismisses a confirm dialog without acting:
// n, esc, or q — the same in every dialog.
func ConfirmCancels(key string) bool {
	switch key {
	case "n", "N", "esc", "q":
		return true
	}
	return false
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

// View returns the dialog as a [Card] for a screen w cells wide (composite it
// over the screen with [Overlay]): message, optional warning, then the two
// choices — the highlighted one as a filled button, the other as plain text —
// and the key hints.
func (d ConfirmDialog) View(w int) string {
	accent := Active.Primary
	if d.Kind == ConfirmDanger {
		accent = Active.Error
	}
	cw := cardWidth(w, 56)
	inner := cw - 4

	parts := []string{lipgloss.NewStyle().Foreground(Active.Text).Render(d.Message), ""}
	if d.Warning != "" {
		parts = append(parts, lipgloss.NewStyle().Foreground(accent).Render(GlyphWarn+" "+d.Warning), "")
	}

	button := func(label string, focused bool) string {
		st := lipgloss.NewStyle().Padding(0, 2)
		if focused {
			return st.Bold(true).Foreground(OnColor(accent)).Background(accent).Render(label)
		}
		return st.Foreground(Active.TextDim).Render(label)
	}
	parts = append(parts,
		button(d.Confirm, d.FocusYes)+"  "+button(d.Cancel, !d.FocusYes),
		"",
		KeyHints(inner, KeyHint{"←→", "choose"}, KeyHint{"↵", "confirm"}, KeyHint{"esc", "cancel"}),
	)
	return Card(d.Title, lipgloss.JoinVertical(lipgloss.Left, parts...), cw, accent)
}
