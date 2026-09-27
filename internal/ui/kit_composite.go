package ui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// kit_composite.go fakes overlay compositing that lipgloss v1 can't do natively,
// so floating cards (palette, help, dialogs) keep the screen they opened over
// visible — dimmed — instead of replacing it with a blank screen.

// Overlay centers card (a [Card]) over a dimmed copy of base, sized to
// width x height. The base is stripped of its styling and recolored to one
// faint tone; the card rows are spliced in at their centered position in full
// color.
func Overlay(base, card string, width, height int) string {
	if width < 1 || height < 1 {
		return card
	}
	faint := lipgloss.NewStyle().Foreground(Active.TextFaint)

	cardLines := strings.Split(card, "\n")
	cw := lipgloss.Width(card)
	x := max(0, (width-cw)/2)
	y := max(0, (height-len(cardLines))/2)
	right := min(width, x+cw)

	bg := strings.Split(base, "\n")
	out := make([]string, height)
	for i := 0; i < height; i++ {
		// Dimmed background row, normalized to exactly `width` cells.
		bgPlain := ""
		if i < len(bg) {
			bgPlain = ansi.Strip(bg[i])
		}
		bgPlain = padCells(bgPlain, width)

		ci := i - y
		if ci < 0 || ci >= len(cardLines) {
			out[i] = faint.Render(bgPlain)
			continue
		}
		// Re-pad each backdrop segment to its exact cell width: ansi.Truncate /
		// TruncateLeft DROP a double-width rune (CJK/emoji in an instance name)
		// that straddles the card edge rather than splitting it, which would
		// shift the row. padCells fills the gap so left + card + right always
		// totals exactly `width`.
		leftBg := faint.Render(padCells(ansi.Truncate(bgPlain, x, ""), x))
		mid := ansi.Truncate(cardLines[ci], right-x, "")
		rightBg := faint.Render(padCells(ansi.TruncateLeft(bgPlain, right, ""), width-right))
		out[i] = leftBg + mid + rightBg
	}
	return strings.Join(out, "\n")
}

// padCells pads or truncates a plain (ANSI-free) string to exactly w display cells.
func padCells(s string, w int) string {
	sw := ansi.StringWidth(s)
	switch {
	case sw > w:
		return ansi.Truncate(s, w, "")
	case sw < w:
		return s + strings.Repeat(" ", w-sw)
	default:
		return s
	}
}
