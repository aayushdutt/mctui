package ui

import (
	"fmt"
	"io"
	"strings"

	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// Pill-style "new" badge (distinct from instance title typography).
var (
	newBadgeNormal = lipgloss.NewStyle().
			Background(lipgloss.Color("#3F3F46")).
			Foreground(lipgloss.Color("#E4E4E7")).
			Padding(0, 1)

	newBadgeSelected = lipgloss.NewStyle().
				Background(lipgloss.Color("#6D28D9")).
				Foreground(lipgloss.Color("#FAFAFA")).
				Padding(0, 1)

	newBadgeDimmed = lipgloss.NewStyle().
			Background(lipgloss.Color("#27272A")).
			Foreground(lipgloss.Color("#52525B")).
			Padding(0, 1)
)

const titleEllipsis = "…"

// homeInstanceDelegate wraps the default list delegate so never-played instances get a separate "new" badge.
type homeInstanceDelegate struct {
	list.DefaultDelegate
}

func (d *homeInstanceDelegate) Render(w io.Writer, m list.Model, index int, item list.Item) {
	ii, ok := item.(instanceItem)
	if !ok {
		d.DefaultDelegate.Render(w, m, index, item)
		return
	}

	s := &d.Styles
	name := ii.instance.Name
	desc := ii.Description()
	showBadge := ii.instance.LastPlayed.IsZero()

	if m.Width() <= 0 {
		return
	}

	textwidth := m.Width() - s.NormalTitle.GetPaddingLeft() - s.NormalTitle.GetPaddingRight()
	badgeReserve := 0
	if showBadge {
		badgeReserve = lipgloss.Width(newBadgeNormal.Render("new")) + 1
	}

	title := ansi.Truncate(name, textwidth-badgeReserve, titleEllipsis)
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

	var (
		isSelected   = index == m.Index()
		emptyFilter  = m.FilterState() == list.Filtering && m.FilterValue() == ""
		isFiltered   = m.FilterState() == list.Filtering || m.FilterState() == list.FilterApplied
		matchedRunes []int
	)

	if isFiltered {
		matchedRunes = m.MatchesForItem(index)
	}

	joinTitle := func(titleLine string, badge lipgloss.Style) string {
		t := titleLine
		if showBadge {
			return lipgloss.JoinHorizontal(lipgloss.Left, t, " ", badge.Render("new"))
		}
		return t
	}

	var titleOut, descOut string

	switch {
	case emptyFilter:
		t := s.DimmedTitle.Render(title)
		titleOut = joinTitle(t, newBadgeDimmed)
		descOut = s.DimmedDesc.Render(desc)

	case isSelected && m.FilterState() != list.Filtering:
		if isFiltered {
			unmatched := s.SelectedTitle.Inline(true)
			matched := unmatched.Inherit(s.FilterMatch)
			title = lipgloss.StyleRunes(title, matchedRunes, matched, unmatched)
		}
		t := s.SelectedTitle.Render(title)
		titleOut = joinTitle(t, newBadgeSelected)
		descOut = s.SelectedDesc.Render(desc)

	default:
		if isFiltered {
			unmatched := s.NormalTitle.Inline(true)
			matched := unmatched.Inherit(s.FilterMatch)
			title = lipgloss.StyleRunes(title, matchedRunes, matched, unmatched)
		}
		t := s.NormalTitle.Render(title)
		titleOut = joinTitle(t, newBadgeNormal)
		descOut = s.NormalDesc.Render(desc)
	}

	if d.ShowDescription {
		_, _ = fmt.Fprintf(w, "%s\n%s", titleOut, descOut)
		return
	}
	_, _ = fmt.Fprintf(w, "%s", titleOut)
}
