package ui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// kit.go is the shared UI vocabulary every screen builds from: one set of
// glyphs, one panel, one key-hint bar, one rule, one header. Screens compose
// these instead of hand-rolling borders, separators, and icons — that's what
// keeps the TUI visually consistent. The look is "bordered panels": rounded,
// titled cards anchor content, accent section headers and rules separate it.

// Monochrome glyph set — consistent cell widths across terminals (unlike color
// emoji). One brand emoji ([BrandGlyph]) is kept for the wordmark.
const (
	GlyphDone    = "✓" // completed / success
	GlyphFail    = "✗" // error / failure
	GlyphRunning = "◐" // in progress
	GlyphPending = "○" // not started
	GlyphPointer = "▸" // selection / section marker
	GlyphDot     = "●" // status dot
	GlyphWarn    = "▲" // warning
	BrandGlyph   = "🎮"
)

// Rule returns a themed horizontal rule (divider) of the given visible width.
func Rule(width int) string {
	if width < 1 {
		return ""
	}
	return lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render(strings.Repeat("─", width))
}

// SectionHeader renders an accent ▸ marker, a title, and a rule filling the rest
// of width — the "Discover"-style header used to label a region.
func SectionHeader(title string, width int) string {
	mark := lipgloss.NewStyle().Foreground(Active.Primary).Render(GlyphPointer + " ")
	label := lipgloss.NewStyle().Bold(true).Foreground(Active.Title).Render(title)
	head := mark + label
	rest := width - lipgloss.Width(head) - 1
	if rest < 0 {
		return head
	}
	return head + " " + Rule(rest)
}

// ScreenHeader renders a consistent screen title with an optional subtitle line.
func ScreenHeader(title, subtitle string) string {
	t := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render(title)
	if subtitle == "" {
		return t
	}
	s := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(subtitle)
	return lipgloss.JoinVertical(lipgloss.Left, t, s)
}

// KeyHint is a single key + action label, e.g. {"↵", "launch"}.
type KeyHint struct{ Key, Label string }

// KeyHints renders a consistent, width-aware hint bar: keys in the accent color,
// labels muted, separated by a middot. Wraps to multiple lines to fit width.
func KeyHints(width int, hints ...KeyHint) string {
	keyStyle := lipgloss.NewStyle().Foreground(Active.Secondary)
	lblStyle := lipgloss.NewStyle().Foreground(Active.TextFaint)
	sep := lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render("  ·  ")

	items := make([]string, 0, len(hints))
	for _, h := range hints {
		item := keyStyle.Render("[" + h.Key + "]")
		if h.Label != "" {
			item += " " + lblStyle.Render(h.Label)
		}
		items = append(items, item)
	}
	return joinWrapped(items, sep, width)
}

// StatusBar lays out left content and right content on one line, filling the gap
// so right is flush to width. Falls back to a single space if they don't fit.
func StatusBar(left, right string, width int) string {
	gap := width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 1 {
		gap = 1
	}
	return left + strings.Repeat(" ", gap) + right
}

// Panel renders body inside a rounded border with the title set into the top
// edge, accent-colored:
//
//	╭─ Title ─────────╮
//	│ body            │
//	╰─────────────────╯
//
// width is the total outer width. Body lines are padded/wrapped to fit.
func Panel(title, body string, width int, accent lipgloss.Color) string {
	if width < 4 {
		width = 4
	}
	border := lipgloss.NewStyle().Foreground(accent)
	inner := width - 2    // cells between the corner glyphs
	contentW := width - 4 // inner minus one space of padding each side

	var top string
	if title != "" {
		// Keep at least the leading dash, two pad spaces, and a closing dash.
		if maxTitle := inner - 4; lipgloss.Width(title) > maxTitle {
			title = ansi.Truncate(title, max(0, maxTitle), "…")
		}
		label := lipgloss.NewStyle().Bold(true).Foreground(accent).Render(" " + title + " ")
		dashes := inner - 1 - lipgloss.Width(label) // one leading ─ before the label
		if dashes < 0 {
			dashes = 0
		}
		top = border.Render("╭─") + label + border.Render(strings.Repeat("─", dashes)+"╮")
	} else {
		top = border.Render("╭" + strings.Repeat("─", inner) + "╮")
	}

	left := border.Render("│") + " "
	right := " " + border.Render("│")
	var rows []string
	block := lipgloss.NewStyle().Width(contentW).Render(body) // pads every line to contentW, wraps long ones
	for _, vl := range strings.Split(block, "\n") {
		rows = append(rows, left+vl+right)
	}

	bottom := border.Render("╰" + strings.Repeat("─", inner) + "╯")
	return strings.Join(append(append([]string{top}, rows...), bottom), "\n")
}

// joinWrapped joins items with sep, wrapping to a new line when the next item
// would exceed width (by visible cell width, ignoring ANSI).
func joinWrapped(items []string, sep string, width int) string {
	if width <= 0 {
		width = 80
	}
	sepW := lipgloss.Width(sep)
	var lines []string
	cur := ""
	curW := 0
	for _, it := range items {
		w := lipgloss.Width(it)
		switch {
		case cur == "":
			cur, curW = it, w
		case curW+sepW+w <= width:
			cur += sep + it
			curW += sepW + w
		default:
			lines = append(lines, cur)
			cur, curW = it, w
		}
	}
	if cur != "" {
		lines = append(lines, cur)
	}
	return strings.Join(lines, "\n")
}
