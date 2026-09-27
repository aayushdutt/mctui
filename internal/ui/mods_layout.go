package ui

import (
	"fmt"

	"github.com/dustin/go-humanize"
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
	modsOuterPadY = 0 // single-shell inset: the leaf view no longer self-pads (AppShellStyle owns it)

	modsHeaderLines = 2 // AppHeader + its bottom margin

	modsPanelBorderV = 2 // rounded border top + bottom (one pane)

	modsSplitInterBlockGaps = 1 // blank line above help (the header's margin is its gap)

	// Right-column stack above the results: the search line, the heading (or a
	// status in its place), and a blank line (buildModrinthChrome).
	modsRightColumnChromeLines = 3
	// The results heading shares the status line, so it adds nothing.
	modsBrowseSectionHdrLines = 0

	// Installed pane: one banner line (empty hint, error, or toast) above the list.
	// (In split layout the pane pads this to the Discover chrome so the two
	// lists' rows line up; that's free, the right column is the taller one.)
	modsLibraryChromeLines = 1
)

// modsFooterItems returns the footer hints for the focused pane — only keys
// that work there, ending in ? help where ? isn't typed into the search field.
func modsFooterItems(focus modsPanel) []KeyHint {
	switch focus {
	case panelInstalled:
		return []KeyHint{{"d", "remove"}, {"→", "discover"}, {"/", "search"}, {"esc", "home"}, {"?", "help"}}
	case panelQuery:
		return []KeyHint{{"↵", "search"}, {"↓", "results"}, {"tab", "next pane"}, {"esc", "done"}}
	default:
		return []KeyHint{{"↵", "install"}, {"←", "installed"}, {"/", "search"}, {"esc", "home"}, {"?", "help"}}
	}
}

// modsInstallingFooterItems is the footer while a mod downloads.
func modsInstallingFooterItems() []KeyHint {
	return []KeyHint{{"esc", "cancel download"}, {"↑↓", "browse"}, {"tab", "next pane"}, {"?", "help"}}
}

// modsFooterVariants are all the footers the screen can show. The height budget
// uses the tallest, so moving focus can never change the layout.
func modsFooterVariants() [][]KeyHint {
	return [][]KeyHint{modsFooterItems(panelInstalled), modsFooterItems(panelQuery), modsFooterItems(panelBrowse), modsInstallingFooterItems()}
}

// modsHelpBodyMaxWidth matches ModsModel.View: modsRenderHelp(..., m.width).
func modsHelpBodyMaxWidth(termW int) int {
	return max(1, termW)
}

// modsRenderHelp renders the footer hint bar through the shared kit. View and
// the height math below both call this, so they can never desync.
func modsRenderHelp(items []KeyHint, width int) string {
	return KeyHints(width, items...)
}

func modsFooterHelpLinesFromItems(items []KeyHint, termW int) int {
	return footerHelpHeight(items, modsHelpBodyMaxWidth(termW))
}

// modsFooterHelpLines is the footer height budget: the tallest variant.
func modsFooterHelpLines(termH, termW int) int {
	h := 0
	for _, items := range modsFooterVariants() {
		h = max(h, modsFooterHelpLinesFromItems(items, termW))
	}
	return h
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

// modsCompactListHeights splits available rows between installed and Modrinth stacks.
func modsCompactListHeights(termH, termW int) (libListH, browseListH int) {
	if termH < 1 {
		return 1, 1
	}
	helpH := modsFooterHelpLines(termH, termW)
	libPaneShell := modsPanelBorderV + modsLibraryChromeLines
	browsePaneShell := modsPanelBorderV + modsRightColumnChromeLines + modsBrowseSectionHdrLines
	stackGaps := 2 // installed→browse, browse→help (the header carries its own gap)
	preList := modsOuterPadY + modsHeaderLines + stackGaps + helpH
	room := termH - preList - libPaneShell - browsePaneShell
	if room < 2 {
		return 1, 1
	}
	// Browse list gets the lion’s share of remaining rows; installed gets about a
	// third — two or three of its two-line rows.
	libListH = max(2, min(9, room/3))
	if room >= 6 {
		libListH = max(3, libListH) // one two-line row + the page dots
	}
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

// formatModHitCount formats Modrinth's total hit count: "5,056", "12.3K",
// "1.2M".
func formatModHitCount(total int) string {
	if total >= 1_000_000 {
		return fmt.Sprintf("%.1fM", float64(total)/1_000_000)
	}
	if total >= 10_000 {
		return fmt.Sprintf("%.1fK", float64(total)/1_000)
	}
	return humanize.Comma(int64(total))
}
