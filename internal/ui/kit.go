package ui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// kit.go is the shared UI vocabulary every screen builds from: one header, one
// key-hint bar, one selection bar, one rule, one small set of glyphs. Screens
// compose these instead of hand-rolling borders, separators, and icons — that's
// what keeps the TUI consistent.
//
// The look: quiet chrome, loud content. Structure comes from whitespace, one
// hairline rule under each pane title, and a three-step gray ramp (content,
// metadata, hints). The accent color only ever means "you are here / this is a
// key": the selection bar, the focused title, key glyphs. Green, amber, and red
// are reserved for state. Boxes are only for things that float (dialogs, the
// palette, help).

// Monochrome glyph set — consistent cell widths across terminals (unlike color
// emoji).
const (
	GlyphDone    = "✓" // completed / success
	GlyphFail    = "✗" // error / failure
	GlyphPending = "○" // not started
	GlyphPointer = "▸" // section marker / menu cursor
	GlyphDot     = "●" // status dot
	GlyphWarn    = "▲" // warning
)

// rowMarker returns the 2-cell leading gutter for a hand-rendered selectable
// row: the accent [SelectMark] plus a space when selected, two blanks
// otherwise. The width is constant so selected and unselected rows stay
// column-aligned, and it matches the pointer the list delegates draw, so every
// selectable row in the app shares one selection cue. Pair it with a bright,
// bold label on the selected row.
func rowMarker(selected bool) string {
	if !selected {
		return "  "
	}
	return selectMark(Active.Primary) + " "
}

// Zone renders a titled content region WITHOUT a box: a bold title line — the
// accent color when focused, muted when not — over a dim [Rule], over the body.
// It is the in-layout replacement for [Panel]: structure comes from the title,
// the rule, and whitespace instead of a rounded border. The title sits in the
// same 2-cell gutter as list rows, so it lines up with their text. Floating
// things (dialogs, palette, help) use [Card].
//
// width is the rule width; body is rendered flush-left beneath it (size the body
// to width before passing it in). An empty title renders just the rule + body.
func Zone(title, body string, width int, focused bool) string {
	return ZoneR(title, "", body, width, focused)
}

// ZoneR is [Zone] with a right-aligned annotation on the title line — a count
// ("5") or a status ("● Ready"). The annotation is dropped if it doesn't fit.
func ZoneR(title, right, body string, width int, focused bool) string {
	// Truncate the title to the pane so a long one (e.g. an accumulating
	// resource-packs breadcrumb) can't widen the Zone past its column and shove
	// the neighbouring pane off the frame.
	// The title sits after a 2-cell gutter (aligned with list-row text).
	room := width - 2
	if right != "" {
		if lipgloss.Width(right)+4 < room {
			room -= lipgloss.Width(right) + 2
		} else {
			right = ""
		}
	}
	if title != "" {
		title = ansi.Truncate(title, max(1, room), "…")
	}

	var head string
	switch {
	case title == "":
		head = ""
	case focused:
		label := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render(title)
		head = "  " + label
	default:
		label := lipgloss.NewStyle().Bold(true).Foreground(Active.TextMuted).Render(title)
		head = "  " + label
	}

	if head != "" && right != "" {
		head = StatusBar(head, right, width)
	}
	parts := make([]string, 0, 3)
	if head != "" {
		parts = append(parts, head, Rule(width))
	}
	if body != "" {
		parts = append(parts, body)
	}
	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}

// ClampWidth truncates every line of s to at most width cells — a defensive
// backstop so no leaf view can push a line past the app frame (e.g. an unwrapped
// hint on a very narrow terminal). Leaf views should still size their content to
// the width; this only guarantees the frame stays intact.
func ClampWidth(s string, width int) string {
	if width < 1 {
		return s
	}
	return lipgloss.NewStyle().MaxWidth(width).Render(s)
}

// ClampHeight keeps at most the first height lines of s — the vertical twin of
// ClampWidth. A view taller than the terminal would otherwise lose its TOP rows
// (bubbletea keeps the bottom), i.e. the header that says where you are.
func ClampHeight(s string, height int) string {
	if height < 1 {
		return s
	}
	if lines := strings.Split(s, "\n"); len(lines) > height {
		return strings.Join(lines[:height], "\n")
	}
	return s
}

// ReplaceLastLine puts line on the last row of a height-row screen (padding
// with blank rows when the view is shorter) — used for app-level prompts.
func ReplaceLastLine(s string, height int, line string) string {
	lines := strings.Split(s, "\n")
	for len(lines) < height {
		lines = append(lines, "")
	}
	lines[len(lines)-1] = line
	return strings.Join(lines, "\n")
}

// QuitWhilePlayingPrompt is the confirmation shown after a first q / ctrl+c
// while a game is still running.
func QuitWhilePlayingPrompt(width int) string {
	return lipgloss.NewStyle().Foreground(Active.Warning).Render(ansi.Truncate(
		GlyphWarn+" Minecraft is still running — press again to quit mctui (the game keeps running; its playtime won't be recorded).",
		max(1, width), "…"))
}

// Rule returns a themed horizontal rule (divider) of the given visible width.
func Rule(width int) string {
	if width < 1 {
		return ""
	}
	return lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render(strings.Repeat("─", width))
}

// SectionHeader renders a title and a rule filling the rest of width — used to
// label a region inside a pane ("Discover ──────").
func SectionHeader(title string, width int) string {
	head := lipgloss.NewStyle().Bold(true).Foreground(Active.Title).Render(title)
	rest := width - lipgloss.Width(head) - 1
	if rest < 0 {
		return head
	}
	return head + " " + Rule(rest)
}

// AppHeader is the one header every screen starts with: the "mctui" wordmark
// and a breadcrumb trail on the left ("mctui › Survival World › Mods"), and
// optional context flush right (account, counts, state). The last crumb — where
// you are — is bright; earlier ones are dim. When both sides don't fit, the
// right side is dropped and the trail truncated, so it's always one line.
func AppHeader(width int, right string, crumbs ...string) string {
	sep := lipgloss.NewStyle().Foreground(Active.TextFaint).Render("  ›  ")
	parts := []string{lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render("mctui")}
	for i, c := range crumbs {
		st := lipgloss.NewStyle().Foreground(Active.TextDim)
		if i == len(crumbs)-1 {
			st = lipgloss.NewStyle().Foreground(Active.TextStrong)
		}
		parts = append(parts, st.Render(c))
	}
	left := strings.Join(parts, sep)
	if right != "" && lipgloss.Width(left)+2+lipgloss.Width(right) <= width {
		return StatusBar(left, right, width)
	}
	return ansi.Truncate(left, max(1, width), "…")
}

// KeyHint is a single key + action label, e.g. {"↵", "launch"}.
type KeyHint struct{ Key, Label string }

// KeyHints renders the width-aware hint bar used by every footer:
//
//	↵ play   n new   / search   ? help
//
// Keys bold in the accent, labels dim, no brackets or bullets — the color does
// the grouping. Wraps to multiple lines to fit width.
func KeyHints(width int, hints ...KeyHint) string {
	keyStyle := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary)
	lblStyle := lipgloss.NewStyle().Foreground(Active.TextDim)
	const sep = "   "

	items := make([]string, 0, len(hints))
	for _, h := range hints {
		item := keyStyle.Render(h.Key)
		if h.Label != "" {
			item += " " + lblStyle.Render(h.Label)
		}
		items = append(items, item)
	}
	return joinWrapped(items, sep, width)
}

// footerHelpHeight is the rendered height of a KeyHints footer (one line, or
// more when it wraps) plus the blank line above it. The mods and resource-packs
// vertical budgets share this with their View, so the two can't drift.
func footerHelpHeight(items []KeyHint, bodyWidth int) int {
	body := KeyHints(bodyWidth, items...)
	lines := 1
	if body != "" {
		lines = strings.Count(body, "\n") + 1
	}
	return 1 + lines // blank line + hint text
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
