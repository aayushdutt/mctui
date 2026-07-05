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

	// rpHeaderLines: title + catalog-version line + rule + margin.
	rpHeaderLines = 4

	// rpPanelBorderV: rounded border top + bottom (one pane).
	rpPanelBorderV = 2

	// rpInterBlockGaps: blank lines under header and above help.
	rpInterBlockGaps = 2

	// rpFooterLines: two-tier key hints.
	rpFooterLines = 2

	// rpStatusLines: the single status/notice row above the panes.
	rpStatusLines = 1

	// rpCartCTAReserve is the height of the cart pane's "Build & Apply" block
	// (a blank spacer line + the CTA line) rendered below the cart list whenever
	// the cart is non-empty. applyPaneListHeights subtracts it from the cart list
	// viewport so the pane stays within the budgeted height.
	rpCartCTAReserve = 2
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
// below/above the lists (the cart "Build & Apply" CTA block and any category
// warning line) so the stacked panes still fit the terminal.
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

// rpScreenHelpItems is the full footer: one line, six hints, ending in `? help`.
// The long tail (→ open, o preview, : palette) lives in the `?` overlay.
func rpScreenHelpItems() []KeyHint {
	return []KeyHint{
		{"space", "toggle"},
		{"/", "search"},
		{"b", "apply"},
		{"c", "clear"},
		{"esc", "home"},
		{"?", "help"},
	}
}

func rpScreenHelpItemsCompact() []KeyHint {
	return []KeyHint{
		{"space", "toggle"},
		{"/", "search"},
		{"b", "apply"},
		{"esc", "home"},
		{"?", "help"},
	}
}

func rpScreenHelpItemsMicro() []KeyHint {
	return []KeyHint{
		{"space", "toggle"},
		{"b", "apply"},
		{"esc", "home"},
		{"?", "help"},
	}
}

// rpHelpItemsPick selects footer tiers so layout math matches what View renders.
func rpHelpItemsPick(termH, termW int) []KeyHint {
	switch {
	case termH >= 42 && termW >= 60:
		return rpScreenHelpItems()
	case termH >= 22:
		return rpScreenHelpItemsCompact()
	default:
		return rpScreenHelpItemsMicro()
	}
}

// rpHelpBodyMaxWidth matches ResourcePacksModel.View: rpRenderHelp(..., width-6).
func rpHelpBodyMaxWidth(termW int) int {
	return max(1, termW-6)
}

// rpRenderHelp renders the footer hint bar through the shared kit. View and the
// height math both call this so they can never desync.
func rpRenderHelp(items []KeyHint, width int) string {
	return KeyHints(width, items...)
}

// rpFooterHelpLines is the rendered footer height (rule + wrapped hints + margin).
func rpFooterHelpLines(termH, termW int) int {
	return footerHelpHeight(rpHelpItemsPick(termH, termW), rpHelpBodyMaxWidth(termW))
}
