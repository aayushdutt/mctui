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

// HelpOverlay renders the full-screen help modal: a bordered card titled with
// the screen name, its binding groups stacked inside, and a footer telling the
// user how to dismiss it. Sized to fit within width×height.
func HelpOverlay(screen string, groups []HelpGroup, width, height int) string {
	accent := Active.Primary

	panelWidth := width - 8
	if panelWidth > 60 {
		panelWidth = 60
	}
	floor := min(40, max(24, width-8))
	if panelWidth < floor {
		panelWidth = floor
	}
	contentW := panelWidth - 4 // inside the Panel border + padding

	keyStyle := lipgloss.NewStyle().Foreground(Active.Secondary)
	descStyle := lipgloss.NewStyle().Foreground(Active.TextMuted)
	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(Active.Title)

	// Widest key cluster across all groups, so the descriptions align in one column.
	keyW := 0
	for _, g := range groups {
		for _, e := range g.Entries {
			if w := lipgloss.Width(e.Keys); w > keyW {
				keyW = w
			}
		}
	}
	if keyW > contentW-6 {
		keyW = max(1, contentW-6)
	}

	var lines []string
	for gi, g := range groups {
		if len(g.Entries) == 0 {
			continue
		}
		if gi > 0 {
			lines = append(lines, "")
		}
		lines = append(lines, titleStyle.Render(g.Title))
		for _, e := range g.Entries {
			pad := keyW - lipgloss.Width(e.Keys)
			if pad < 0 {
				pad = 0
			}
			row := keyStyle.Render(e.Keys) + strings.Repeat(" ", pad) + "  " + descStyle.Render(e.Desc)
			lines = append(lines, row)
		}
	}

	body := strings.Join(lines, "\n")
	panel := Panel(screen+" · keys", body, panelWidth, accent)

	hint := lipgloss.NewStyle().MarginTop(1).Render(KeyHints(
		panelWidth,
		KeyHint{"?", "close"},
		KeyHint{"esc", "close"},
	))

	box := lipgloss.JoinVertical(lipgloss.Left, panel, hint)
	return lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, box)
}
