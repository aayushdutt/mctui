package ui

import (
	"fmt"
	"io"
	"strings"

	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

const titleEllipsis = "…"

// rowGutter is the width of the leading selection gutter every instance row
// reserves: the [SelectMark] pointer plus a space on the selected row's first
// line, two blanks everywhere else — so all row text starts in the same column.
const rowGutter = 2

// homeInstanceDelegate renders the Instances sidebar. Each instance is two
// lines — name, then "version · loader · last played" — and the selected row
// is marked only by the accent [SelectMark] pointer before its name (bright,
// bold name; the metadata stays quiet). No icons, pills, or colored text: color
// is reserved for the bar, the "Playing" state, and filter matches.
type homeInstanceDelegate struct {
	list.DefaultDelegate

	// unfocused hides the selection while keyboard focus is in the detail pane,
	// so only the focused pane shows a selected row (the detail pane's title
	// already names the instance).
	unfocused bool
}

// bar returns the 2-cell gutter for a row's first line.
func (d *homeInstanceDelegate) bar(selected bool) string {
	if !selected {
		return strings.Repeat(" ", rowGutter)
	}
	return selectMark(Active.Primary) + " "
}

func (d *homeInstanceDelegate) Render(w io.Writer, m list.Model, index int, item list.Item) {
	width := m.Width()
	if width <= rowGutter {
		return
	}
	textW := width - rowGutter
	filtering := m.FilterState() == list.Filtering
	selected := index == m.Index() && !filtering && !d.unfocused
	// While typing an empty filter every row dims (nothing is selectable yet).
	dimmed := filtering && m.FilterValue() == ""

	if _, ok := item.(addInstanceItem); ok {
		// "+ New instance": an ordinary row that takes the same bar when selected.
		style := lipgloss.NewStyle().Foreground(Active.TextMuted)
		if selected {
			style = style.Foreground(Active.TextStrong).Bold(true)
		}
		_, _ = fmt.Fprint(w, d.bar(selected)+style.Render(ansi.Truncate("+ New instance", textW, titleEllipsis)))
		return
	}
	ii, ok := item.(instanceItem)
	if !ok {
		d.DefaultDelegate.Render(w, m, index, item)
		return
	}

	// Title: the name, plus a quiet "new" tag for never-played instances.
	titleStyle := lipgloss.NewStyle().Foreground(Active.Text)
	switch {
	case selected:
		titleStyle = titleStyle.Foreground(Active.TextStrong).Bold(true)
	case dimmed:
		titleStyle = titleStyle.Foreground(Active.TextMuted)
	}
	tag := ""
	if ii.instance.LastPlayed.IsZero() && !ii.playing {
		tag = "  " + lipgloss.NewStyle().Foreground(Active.Secondary).Render("new")
	}
	name := ansi.Truncate(ii.instance.Name, textW-lipgloss.Width(tag), titleEllipsis)
	title := titleStyle.Render(name)
	if m.FilterState() != list.Unfiltered {
		matched := titleStyle.Inline(true).Inherit(d.Styles.FilterMatch)
		title = lipgloss.StyleRunes(name, m.MatchesForItem(index), matched, titleStyle.Inline(true))
	}

	// Meta: "1.21.4 · Fabric · 2h ago", or the Playing state leading in accent.
	metaStyle := lipgloss.NewStyle().Foreground(Active.TextDim)
	if dimmed {
		metaStyle = metaStyle.Foreground(Active.TextFaint)
	}
	meta := metaStyle.Render(ansi.Truncate(ii.meta(), textW, titleEllipsis))
	if ii.playing {
		lead := GlyphDot + " Playing"
		rest := ansi.Truncate(" · "+ii.instance.Version+" · "+loaderName(ii.instance.Loader), max(0, textW-lipgloss.Width(lead)), titleEllipsis)
		meta = lipgloss.NewStyle().Foreground(Active.Success).Render(lead) + metaStyle.Render(rest)
	}

	gutter := d.bar(selected)
	if d.ShowDescription {
		_, _ = fmt.Fprintf(w, "%s%s%s\n%s%s", gutter, title, tag, strings.Repeat(" ", rowGutter), meta)
		return
	}
	_, _ = fmt.Fprintf(w, "%s%s%s", gutter, title, tag)
}
