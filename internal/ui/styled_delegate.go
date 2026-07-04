package ui

import (
	"fmt"
	"io"
	"strings"

	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// styledTitleItem is a list item whose title carries lipgloss styling that
// bubbles' default filter-match highlighter would corrupt. bubbles highlights
// matches with lipgloss.StyleRunes(title, …), which is NOT ANSI-aware: it walks
// the title rune-by-rune — counting the ESC/'['/digits of any embedded SGR
// sequence as ordinary runes — and re-wraps matched ranges, tearing those
// sequences apart and leaking their bodies (e.g. "38;2;241;250;140m") onto the
// screen. The corruption only appears while a filter is active, because that is
// the only time StyleRunes runs.
//
// Items split their title into a styled prefix (a marker/swatch, rendered
// verbatim and never highlighted) and a plain name. styledTitleDelegate then
// highlights matches over the PLAIN name and prepends the prefix, so the
// prefix's ANSI stays intact and match underlines land at the right offsets.
type styledTitleItem interface {
	list.Item
	// titleParts returns the styled prefix (may contain ANSI), the plain,
	// unstyled name, and the base style to paint the name with. Title() should
	// return prefix + nameStyle.Render(name) so unfiltered rendering is unchanged.
	titleParts() (prefix, name string, nameStyle lipgloss.Style)
}

// styledTitleDelegate wraps the default list delegate so styled-title rows get
// ANSI-safe filter-match highlighting. Plain rows (anything not implementing
// styledTitleItem) fall through to the default delegate untouched.
type styledTitleDelegate struct {
	list.DefaultDelegate
}

func (d *styledTitleDelegate) Render(w io.Writer, m list.Model, index int, item list.Item) {
	it, ok := item.(styledTitleItem)
	if !ok {
		d.DefaultDelegate.Render(w, m, index, item)
		return
	}
	if m.Width() <= 0 {
		return
	}
	s := &d.Styles

	prefix, name, nameStyle := it.titleParts()
	desc := ""
	if di, ok := item.(list.DefaultItem); ok {
		desc = di.Description()
	}

	textwidth := m.Width() - s.NormalTitle.GetPaddingLeft() - s.NormalTitle.GetPaddingRight()

	var (
		isSelected   = index == m.Index()
		emptyFilter  = m.FilterState() == list.Filtering && m.FilterValue() == ""
		isFiltered   = m.FilterState() == list.Filtering || m.FilterState() == list.FilterApplied
		matchedRunes []int
	)
	if isFiltered {
		matchedRunes = m.MatchesForItem(index)
	}

	// Highlight matches over the PLAIN name, then prepend the styled prefix.
	// StyleRunes is safe here because `name` has no embedded ANSI to tear; the
	// prefix is concatenated afterwards so it is never passed through StyleRunes.
	styledName := nameStyle.Render(name)
	if isFiltered && !emptyFilter && len(matchedRunes) > 0 {
		// Matched runes index into FilterValue(), which may append a slug/id after
		// the name. Keep only matches that fall within the visible name.
		nameLen := len([]rune(name))
		kept := make([]int, 0, len(matchedRunes))
		for _, r := range matchedRunes {
			if r >= 0 && r < nameLen {
				kept = append(kept, r)
			}
		}
		unmatched := nameStyle.Inline(true)
		matched := unmatched.Inherit(s.FilterMatch)
		styledName = lipgloss.StyleRunes(name, kept, matched, unmatched)
	}

	title := ansi.Truncate(prefix+styledName, textwidth, titleEllipsis)

	if d.ShowDescription {
		var lines []string
		for i, line := range strings.Split(desc, "\n") {
			if i >= d.Height()-1 {
				break
			}
			lines = append(lines, ansi.Truncate(line, textwidth, titleEllipsis))
		}
		desc = strings.Join(lines, "\n")
	}

	var titleOut, descOut string
	switch {
	case emptyFilter:
		titleOut = s.DimmedTitle.Render(title)
		descOut = s.DimmedDesc.Render(desc)
	case isSelected && m.FilterState() != list.Filtering:
		titleOut = s.SelectedTitle.Render(title)
		descOut = s.SelectedDesc.Render(desc)
	default:
		titleOut = s.NormalTitle.Render(title)
		descOut = s.NormalDesc.Render(desc)
	}

	if d.ShowDescription {
		_, _ = fmt.Fprintf(w, "%s\n%s", titleOut, descOut)
		return
	}
	_, _ = fmt.Fprintf(w, "%s", titleOut)
}
