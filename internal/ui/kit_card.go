package ui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// Card renders a floating panel — the command palette, help, dialogs, the
// sign-in card — as a cleared rectangle with a bold title over a rule, the
// body, and a closing rule:
//
//	Title
//	──────────────────
//
//	body…
//
//	──────────────────
//
// It uses only text and horizontal rules on purpose. Terminals that add line
// spacing paint neither vertical strokes (│ ┃) nor background colors into the
// gap between rows, so a bordered or filled box shows dashes or stripes;
// horizontal rules and plain space look the same everywhere. Composited over a
// dimmed screen ([Overlay]), the cleared area reads as the card. The result is
// exactly width cells wide.
func Card(title, body string, width int, accent lipgloss.Color) string {
	width = max(8, width)
	inner := width - 4
	lines := []string{""}
	if title != "" {
		t := lipgloss.NewStyle().Bold(true).Foreground(accent).Render(ansi.Truncate(title, inner, "…"))
		lines = append(lines, t)
	}
	lines = append(lines, Rule(inner), "")
	if body != "" {
		lines = append(lines, strings.Split(lipgloss.NewStyle().Width(inner).Render(body), "\n")...)
	}
	lines = append(lines, "", Rule(inner), "")
	for i, l := range lines {
		lines[i] = "  " + l + strings.Repeat(" ", max(0, inner-ansi.StringWidth(l))) + "  "
	}
	return strings.Join(lines, "\n")
}

// cardWidth picks a card width for a screen: want, but never wider than the
// screen allows (8 cells of margin) and never narrower than a readable floor.
func cardWidth(screenW, want int) int {
	w := min(want, screenW-8)
	return max(min(want, max(20, screenW-2)), w)
}

// SelectMark is the selection pointer: an accent "›" on the first line of the
// selected row, with the row's text bright and bold. It's a single glyph on one
// line on purpose — a bar running down a multi-line row breaks into dashes in
// terminals with line spacing.
const SelectMark = "›"

// selectMark renders the pointer in color.
func selectMark(color lipgloss.Color) string {
	return lipgloss.NewStyle().Bold(true).Foreground(color).Render(SelectMark)
}
