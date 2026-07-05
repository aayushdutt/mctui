package ui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// kit_composite.go fakes overlay compositing that lipgloss v1 can't do natively.
// Every dialog / palette / help overlay used to render on a BLANK screen, so
// opening one erased all context (which instance you were deleting, which screen
// you were on). Composite keeps the context: it dims the underlying view and
// splices the overlay's box over the center.

// Composite renders `overlay` (a full-screen, centered box on blank padding — the
// output of lipgloss.Place) over a dimmed copy of `background`, sized to width x
// height. The background is stripped of its own styling and recolored to a single
// faint tone; each overlay row's non-blank span (its box — contiguous because the
// overlays are bordered) is spliced over that dim backdrop, so the box shows in
// full color with the context ghosted behind it.
func Composite(background, overlay string, width, height int) string {
	if width < 1 || height < 1 {
		return overlay
	}
	faint := lipgloss.NewStyle().Foreground(Active.TextFaint)

	bg := strings.Split(background, "\n")
	ov := strings.Split(overlay, "\n")

	out := make([]string, height)
	for i := 0; i < height; i++ {
		// Dimmed background row, normalized to exactly `width` cells.
		bgPlain := ""
		if i < len(bg) {
			bgPlain = ansi.Strip(bg[i])
		}
		bgPlain = padCells(bgPlain, width)

		ovRow := ""
		if i < len(ov) {
			ovRow = ov[i]
		}
		ovPlain := ansi.Strip(ovRow)
		trimmed := strings.TrimRight(ovPlain, " ")
		if trimmed == "" {
			// No overlay content on this row — pure dimmed background.
			out[i] = faint.Render(bgPlain)
			continue
		}
		// The box occupies cells [left, right) on this row.
		left := len(ovPlain) - len(strings.TrimLeft(ovPlain, " ")) // leading spaces are 1 cell each
		right := ansi.StringWidth(trimmed)
		if right > width {
			right = width
		}
		// Re-pad each backdrop segment to its exact cell width: ansi.Truncate /
		// TruncateLeft DROP a double-width rune (CJK/emoji in an instance name)
		// that straddles the box edge rather than splitting it, which would shift
		// the dim row one cell. padCells fills the gap so left + box + right always
		// totals exactly `width`.
		leftBg := faint.Render(padCells(ansi.Truncate(bgPlain, left, ""), left))
		box := ansi.Cut(ovRow, left, right)
		rightBg := faint.Render(padCells(ansi.TruncateLeft(bgPlain, right, ""), width-right))
		out[i] = leftBg + box + rightBg
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
