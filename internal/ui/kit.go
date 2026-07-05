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
	GlyphPointer = "▸" // section marker / menu cursor
	GlyphDot     = "●" // status dot
	GlyphWarn    = "▲" // warning
	BrandGlyph   = "🎮"

	// SelectBar is the accent bar that marks the selected/hovered row — the one
	// selection idiom shared across the whole UI. The bubbles list delegate
	// paints it as a thick left border (see ThemeListDelegate); hand-rendered
	// rows draw it via [rowMarker]. Keeping a single glyph here is what makes the
	// Instances list, the detail rows, and the overlay menus all highlight the
	// same way.
	SelectBar = "┃"
)

// rowMarker returns the 2-cell leading gutter for a hand-rendered selectable
// row: an accent [SelectBar] when selected, two blanks otherwise. The width is
// constant so selected and unselected rows stay column-aligned, and it mirrors
// the bubbles delegate's selected left bar so every selectable row in the app
// shares one selection cue. Pair it with a bold/accent label on the selected
// row.
func rowMarker(selected bool) string {
	if !selected {
		return "  "
	}
	return lipgloss.NewStyle().Foreground(Active.Primary).Bold(true).Render(SelectBar) + " "
}

// ActionButton renders an inline action affordance (e.g. "Add mods…") as a
// button that is visually distinct from selectable list rows: a leading "+"
// badge inside a pill. Unfocused it is an outlined "ghost" button — accent text
// on a faint fill (the same chip language as the list's "new" badge); focused it
// fills with the accent color like the primary Play button, signalling it is the
// active target. Single line, so it is safe inside a tight row list. This is the
// one place inline actions get their identity — route every "+ Add…" style row
// through it so they read the same everywhere.
func ActionButton(label string, focused bool) string {
	st := lipgloss.NewStyle().Bold(true).Padding(0, 1)
	if focused {
		st = st.Foreground(OnColor(Active.Primary)).Background(Active.Primary)
	} else {
		st = st.Foreground(Active.Primary).Background(Active.BorderFaint)
	}
	return st.Render("+ " + label)
}

// Zone renders a titled content region WITHOUT a box: a bold title line — an
// accent ┃ focus bar + accent label when focused, a muted label (two-space
// gutter, no bar) when not — over a dim [Rule], over the body. It is the
// in-layout replacement for [Panel]: structure comes from the title, the rule,
// and whitespace instead of a rounded border, so it reclaims the border's two
// columns and bottom row and keeps the focus cue consistent with the ┃ selection
// idiom. Panel stays for FLOATING overlays (dialogs, palette, help), where the
// box's containment is real information.
//
// width is the rule width; body is rendered flush-left beneath it (size the body
// to width before passing it in). An empty title renders just the rule + body.
func Zone(title, body string, width int, focused bool) string {
	// Truncate the title to the pane so a long one (e.g. an accumulating
	// resource-packs breadcrumb) can't widen the Zone past its column and shove
	// the neighbouring pane off the frame — Panel used to clamp this; Zone must too.
	// The 2-cell gutter is the ┃ + space (focused) or two spaces (not).
	if title != "" {
		title = ansi.Truncate(title, max(1, width-2), "…")
	}

	var head string
	switch {
	case title == "":
		head = ""
	case focused:
		bar := lipgloss.NewStyle().Foreground(Active.Primary).Bold(true).Render(SelectBar)
		label := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render(title)
		head = bar + " " + label
	default:
		label := lipgloss.NewStyle().Bold(true).Foreground(Active.TextMuted).Render(title)
		head = "  " + label
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

// Pill renders a label as a ghost→filled pill: accent text on a faint fill,
// filling with the accent color (WCAG-safe text via OnColor) when focused. Same
// affordance as [ActionButton] but without the leading "+", for standalone
// actions like Save.
func Pill(label string, focused bool) string {
	st := lipgloss.NewStyle().Bold(true).Padding(0, 2)
	if focused {
		st = st.Foreground(OnColor(Active.Primary)).Background(Active.Primary)
	} else {
		st = st.Foreground(Active.Primary).Background(Active.BorderFaint)
	}
	return st.Render(label)
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

// footerHelpHeight is the rendered height of a one-line (or wrapped) KeyHints
// footer plus its leading rule and top margin. The mods and resource-packs
// vertical budgets share this so the "single shell owns the insets" layout keeps
// one source of truth for footer height instead of two hand-copied formulas.
func footerHelpHeight(items []KeyHint, bodyWidth int) int {
	body := KeyHints(bodyWidth, items...)
	lines := 1
	if body != "" {
		lines = strings.Count(body, "\n") + 1
	}
	return 1 + lines + 1 // rule + hint text + top margin
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
