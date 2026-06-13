package ui

import (
	"fmt"
	"strings"
)

// Mods screen vertical layout: ModsModel.SetSize derives bubble list heights from the terminal so
// split/stacked panes and the footer fit. See mods_layout_test.go for size invariants.
//
// Recommended minimum: about 32×40 (stacked mode uses more vertical chrome than split).
// Very long status lines can still wrap past the nominal right-column budget (7 rows).
//
// Vertical budget for the mods screen (split + compact). Tuned against View(): outer
// padding, header block, panel borders, chrome above lists, gaps, and help.
const (
	modsOuterPadY = 2 // lipgloss.NewStyle().Padding(1, 2) top + bottom on the screen block

	modsHeaderLines = 4 // title + context + rule + header margin

	modsPanelBorderV = 2 // rounded border top + bottom (one pane)

	modsSplitInterBlockGaps = 2 // blank lines: under header, above help

	// Upper bound for right-column stack above the Modrinth list (query, status,
	// meta, hint). The "Discover" title now lives in the panel border, not here.
	modsRightColumnChromeLines = 7
	// Single-line kit SectionHeader under that stack (▸ title ──).
	modsBrowseSectionHdrLines = 1

	// Installed pane: banner only (note, dialog, or toast). Its title lives in
	// the panel border now.
	modsLibraryChromeLines = 3

	modsHelpRuleLines = 1
	modsHelpMargin    = 1
)

// modsScreenHelpItems is the full footer when there is enough terminal space.
func modsScreenHelpItems() []KeyHint {
	return []KeyHint{
		{"tab", "switch pane"},
		{"←→", "panes"},
		{"/", "search"},
		{"↵", "add"},
		{"d", "remove"},
		{"r", "refresh"},
		{"esc", "home"},
	}
}

func modsScreenHelpItemsCompact() []KeyHint {
	return []KeyHint{
		{"tab", "pane"},
		{"/", "search"},
		{"↵", "add"},
		{"d", "remove"},
		{"esc", "home"},
	}
}

func modsScreenHelpItemsMicro() []KeyHint {
	return []KeyHint{
		{"tab", "pane"},
		{"/", "search"},
		{"↵", "add"},
		{"esc", "home"},
	}
}

// modsHelpItemsPick selects footer tiers so layout math matches what View renders.
func modsHelpItemsPick(termH, termW int) []KeyHint {
	switch {
	case termH >= 42 && termW >= 52:
		return modsScreenHelpItems()
	case termH >= 22:
		return modsScreenHelpItemsCompact()
	default:
		return modsScreenHelpItemsMicro()
	}
}

// modsHelpBodyMaxWidth matches ModsModel.View: modsRenderHelp(..., m.width-6).
func modsHelpBodyMaxWidth(termW int) int {
	return max(1, termW-6)
}

// modsRenderHelp renders the footer hint bar through the shared kit. View and
// the height math below both call this, so they can never desync.
func modsRenderHelp(items []KeyHint, width int) string {
	return KeyHints(width, items...)
}

func modsFooterHelpLinesFromItems(items []KeyHint, termW int) int {
	body := modsRenderHelp(items, modsHelpBodyMaxWidth(termW))
	textLines := 1
	if body != "" {
		textLines = strings.Count(body, "\n") + 1
	}
	return modsHelpRuleLines + textLines + modsHelpMargin
}

func modsFooterHelpLines(termH, termW int) int {
	return modsFooterHelpLinesFromItems(modsHelpItemsPick(termH, termW), termW)
}

// modsLayoutSplitFixedLines is vertical space (in rows) used in split layout before list viewports and help.
func modsLayoutSplitFixedLines() int {
	return modsOuterPadY + modsHeaderLines + modsSplitInterBlockGaps + modsPanelBorderV +
		modsRightColumnChromeLines + modsBrowseSectionHdrLines
}

// modsSplitListViewportHeight returns the bubble list height for each column in split layout.
func modsSplitListViewportHeight(termH, termW int) int {
	if termH < 1 {
		return 1
	}
	fixed := modsLayoutSplitFixedLines()
	helpH := modsFooterHelpLines(termH, termW)
	listH := termH - fixed - helpH
	// Footer tier matches View; only clamp list when chrome or wrapping leaves no room.
	if fixed+helpH > termH {
		return 1
	}
	if listH < 1 {
		return 1
	}
	return listH
}

// modsCompactFooterItems matches footer choice in compact (stacked) layout for SetSize and View.
func modsCompactFooterItems(termH, termW int) []KeyHint {
	if termH >= 40 {
		return modsHelpItemsPick(termH, termW)
	}
	return modsScreenHelpItemsMicro()
}

// modsCompactListHeights splits available rows between installed and Modrinth stacks.
func modsCompactListHeights(termH, termW int) (libListH, browseListH int) {
	if termH < 1 {
		return 1, 1
	}
	helpItems := modsCompactFooterItems(termH, termW)
	helpH := modsFooterHelpLinesFromItems(helpItems, termW)
	libPaneShell := modsPanelBorderV + modsLibraryChromeLines
	browsePaneShell := modsPanelBorderV + modsRightColumnChromeLines + modsBrowseSectionHdrLines
	stackGaps := 3 // header→installed, installed→browse, browse→help
	preList := modsOuterPadY + modsHeaderLines + stackGaps + helpH
	room := termH - preList - libPaneShell - browsePaneShell
	if room < 2 {
		return 1, 1
	}
	// Browse list gets the lion’s share of remaining rows.
	libListH = max(2, min(6, max(1, room/4)))
	browseListH = room - libListH
	if browseListH < 2 {
		browseListH = 2
		libListH = max(1, room-browseListH)
	}
	if libListH+browseListH > room {
		browseListH = max(1, room/2)
		libListH = max(1, room-browseListH)
	}
	return libListH, browseListH
}

func formatModHitCount(total int) string {
	if total >= 1_000_000 {
		return fmt.Sprintf("%.1fM", float64(total)/1_000_000)
	}
	if total >= 10_000 {
		return fmt.Sprintf("%.1fK", float64(total)/1_000)
	}
	return fmt.Sprintf("%d", total)
}
