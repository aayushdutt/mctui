package ui

import (
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/lipgloss"
)

// kit_help.go is the shared `?` help overlay: a floating, bordered card that
// lists the active screen's bindings grouped by intent (Navigate / Act /
// Screens / General). Every screen builds its groups from the SAME source it
// binds keys from (its keyMap structs, via [HelpEntryFor]), so the overlay can
// never advertise a key the screen doesn't actually handle.

// HelpEntry is one key (or key cluster) and the action it performs.
type HelpEntry struct {
	Keys string
	Desc string
}

// HelpGroup is a titled cluster of entries, e.g. "Navigate".
type HelpGroup struct {
	Title   string
	Entries []HelpEntry
}

// HelpEntryFor lifts a bubbles key.Binding into a HelpEntry using the binding's
// own registered help text — the honest path: the same struct that matches the
// key supplies its documentation.
func HelpEntryFor(b key.Binding) HelpEntry {
	h := b.Help()
	return HelpEntry{Keys: h.Key, Desc: h.Desc}
}

// helpGroup is a small builder that skips bindings with empty help so a screen
// can pass its whole keyMap without curating out the unlabeled ones.
func helpGroup(title string, bindings ...key.Binding) HelpGroup {
	g := HelpGroup{Title: title}
	for _, b := range bindings {
		h := b.Help()
		if h.Key == "" {
			continue
		}
		g.Entries = append(g.Entries, HelpEntry{Keys: h.Key, Desc: h.Desc})
	}
	return g
}

// HelpCard renders the `?` key reference as a [Card] for a width×height screen
// (composite it over the screen with [Overlay]): the screen's binding groups,
// keys in the accent and descriptions dim, with a close hint at the bottom.
// Groups stack in one column; when that's taller than the screen they're laid
// out in two balanced columns instead of being clipped.
func HelpCard(screen string, groups []HelpGroup, width, height int) string {
	keyStyle := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary)
	descStyle := lipgloss.NewStyle().Foreground(Active.TextDim)
	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(Active.Title)

	// Widest key cluster across all groups, so the descriptions align in one column.
	keyW := 0
	for _, g := range groups {
		for _, e := range g.Entries {
			keyW = max(keyW, lipgloss.Width(e.Keys))
		}
	}

	var blocks []string
	for _, g := range groups {
		if len(g.Entries) == 0 {
			continue
		}
		lines := []string{titleStyle.Render(g.Title)}
		for _, e := range g.Entries {
			pad := max(0, keyW-lipgloss.Width(e.Keys))
			lines = append(lines, keyStyle.Render(e.Keys)+strings.Repeat(" ", pad)+"  "+descStyle.Render(e.Desc))
		}
		blocks = append(blocks, strings.Join(lines, "\n"))
	}

	body := strings.Join(blocks, "\n\n")
	colW := lipgloss.Width(body)
	// Card chrome: 1 padding row + title + blank above; blank + hint + padding below.
	const chrome = 6
	if lipgloss.Height(body)+chrome > height && len(blocks) > 1 && 2*colW+4+4 <= width {
		body = twoColumns(blocks, colW)
	}

	hint := KeyHints(lipgloss.Width(body), KeyHint{"esc", "close"})
	body = body + "\n\n" + hint
	cw := min(max(lipgloss.Width(body)+4, 40), max(24, width-4))
	return Card(screen+" · keys", body, cw, Active.Primary)
}

// twoColumns splits blocks into a left and right column of roughly equal
// height and joins them side by side.
func twoColumns(blocks []string, colW int) string {
	total := 0
	for _, b := range blocks {
		total += lipgloss.Height(b) + 1
	}
	var left, right []string
	h := 0
	for _, b := range blocks {
		if h < total/2 || len(left) == 0 {
			left = append(left, b)
			h += lipgloss.Height(b) + 1
		} else {
			right = append(right, b)
		}
	}
	col := lipgloss.NewStyle().Width(colW)
	return lipgloss.JoinHorizontal(lipgloss.Top,
		col.Render(strings.Join(left, "\n\n")), "    ",
		col.Render(strings.Join(right, "\n\n")))
}
