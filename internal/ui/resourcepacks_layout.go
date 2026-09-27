package ui

// Resource-packs screen vertical layout. ResourcePacksModel.SetSize derives
// bubble list heights from the terminal so split/stacked panes and the footer
// fit. Mirrors mods_layout.go; a size-invariant test guards the budget
// (resourcepacks_layout_test.go).
//
// Vertical budget for the resource-packs screen. Tuned against View(): header
// block, panel borders, chrome above lists, gaps, and help.
const (
	// rpOuterPadY is 0: the leaf view no longer self-pads (single-shell inset —
	// AppShellStyle owns the top/bottom padding), so nothing is reserved here.
	rpOuterPadY = 0

	// rpHeaderLines: AppHeader + its bottom margin.
	rpHeaderLines = 2

	// rpPanelBorderV: rounded border top + bottom (one pane).
	rpPanelBorderV = 2

	// rpInterBlockGaps: blank lines under header and above help.
	rpInterBlockGaps = 2

	// rpFooterLines: two-tier key hints.
	rpFooterLines = 2

	// rpStatusLines: the single status/notice row above the panes.
	rpStatusLines = 1
)

// rpCategoryWarningReserve returns the extra lines the packs pane needs for a
// category warning line (1 when warning text is present, else 0). packsPane
// prepends the warning above the pack list, so the pack list viewport must shrink
// by this much to keep the pane within budget.
func rpCategoryWarningReserve(warning string) int {
	if warning == "" {
		return 0
	}
	return 1
}

// rpListViewportHeight returns the height available for each list pane given the
// terminal height/width, in split mode. Mirrors modsSplitListViewportHeight: the
// fixed chrome and the actual footer height are subtracted from the terminal.
func rpListViewportHeight(h, w int) int {
	if h < 1 {
		return 1
	}
	fixed := rpOuterPadY + rpHeaderLines + rpPanelBorderV + rpInterBlockGaps + rpStatusLines
	helpH := rpFooterHelpLines(h, w)
	avail := h - fixed - helpH
	if avail < 1 {
		return 1
	}
	return avail
}

// rpCompactListHeights returns the per-pane heights when stacked (narrow width).
// Three panes share the vertical budget; the pack list (the working surface)
// gets the largest share. extraReserve is the height of in-panel chrome rendered
// above the lists (a category warning line) so the stacked panes still fit the
// terminal.
func rpCompactListHeights(h, w, extraReserve int) (catH, packH, cartH int) {
	if h < 1 {
		return 1, 1, 1
	}
	helpH := rpFooterHelpLines(h, w)
	// Three panel shells (border × 3) + stacking gaps (header→cat, cat→pack,
	// pack→cart, cart→help = 4).
	shells := rpPanelBorderV * 3
	gaps := 4
	pre := rpOuterPadY + rpHeaderLines + rpStatusLines + helpH + shells + gaps + extraReserve
	room := h - pre
	if room < 3 {
		return 1, 1, 1
	}
	catH = max(2, min(6, room/4))
	cartH = max(2, min(6, room/4))
	packH = room - catH - cartH
	if packH < 2 {
		packH = 2
		catH = max(1, (room-packH)/2)
		cartH = max(1, room-packH-catH)
	}
	return catH, packH, cartH
}

// rpFooterState is what the resource-packs footer describes.
type rpFooterState int

const (
	rpFooterCategories rpFooterState = iota
	rpFooterPacks
	rpFooterCart
	rpFooterSearch   // global search results
	rpFooterFilter   // typing a filter
	rpFooterApplying // Build & Apply running
	rpFooterLoadErr  // the catalog failed to load
)

// rpFooterItems returns the hints for a footer state — only keys that work in
// it, ending in ? help where ? isn't typed.
func rpFooterItems(st rpFooterState) []KeyHint {
	switch st {
	case rpFooterCategories:
		return []KeyHint{{"↵", "open"}, {"/", "search"}, {"b", "build & apply"}, {"esc", "home"}, {"?", "help"}}
	case rpFooterCart:
		return []KeyHint{{"space", "remove"}, {"b", "build & apply"}, {"c", "clear"}, {"esc", "home"}, {"?", "help"}}
	case rpFooterSearch:
		return []KeyHint{{"↵", "go to pack"}, {"esc", "exit search"}}
	case rpFooterFilter:
		return []KeyHint{{"↵", "apply"}, {"esc", "cancel"}}
	case rpFooterApplying:
		return []KeyHint{{"esc", "cancel build"}}
	case rpFooterLoadErr:
		return []KeyHint{{"r", "retry"}, {"esc", "home"}, {"?", "help"}}
	default:
		return []KeyHint{{"space", "toggle"}, {"/", "search"}, {"b", "build & apply"}, {"esc", "home"}, {"?", "help"}}
	}
}

// rpHelpBodyMaxWidth matches ResourcePacksModel.View: rpRenderHelp(..., width-6).
func rpHelpBodyMaxWidth(termW int) int {
	return max(1, termW)
}

// rpRenderHelp renders the footer hint bar through the shared kit. View and the
// height math both call this so they can never desync.
func rpRenderHelp(items []KeyHint, width int) string {
	return KeyHints(width, items...)
}

// rpFooterHelpLines is the footer height budget: the tallest of every footer
// the screen can show, so a state change can never move the layout.
func rpFooterHelpLines(termH, termW int) int {
	h := 0
	for st := rpFooterCategories; st <= rpFooterLoadErr; st++ {
		h = max(h, footerHelpHeight(rpFooterItems(st), rpHelpBodyMaxWidth(termW)))
	}
	return h
}
