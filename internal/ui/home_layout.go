package ui

// Home screen layout math. HomeModel.SetSize derives the instance-list viewport
// height and the two-column widths from the already-padded content size handed in
// by the app shell. See home_layout_test.go for the size invariants.
//
// CRITICAL: the home view receives the ALREADY-PADDED content area (the app shell
// in internal/app/app.go subtracts its padding once before calling SetSize). So,
// unlike mods_view.go which does `m.width-8`, the home budget constants must NOT
// include any outer-padding term — m.width/m.height are the full drawable area.

const (
	// homeSplitMinWidth is the breakpoint: at or above this content width the home
	// screen renders the two-column master/detail layout; below it falls back to a
	// single full-width list plus a one-line summary. 76 is the content width of
	// an 80-column terminal (the app shell pads 2 each side), so the common
	// 80x24 gets both columns.
	homeSplitMinWidth = 76

	// homeTitleLines is the StatusBar title row above the columns (screen title
	// left, instance count right).
	homeTitleLines = 1

	// homeTitleGapLines is the blank line between the title row and the columns.
	homeTitleGapLines = 1

	// homePanelBorderV is the column header height above the list viewport: the
	// Zone title line + its dim rule (2 rows) — the same height the rounded Panel
	// border used, so the vertical budget is unchanged by the de-box.
	homePanelBorderV = 2

	// homeColumnGap is the gap (in cells) between the list and detail columns.
	homeColumnGap = 2

	// home list pane width bias: the list column is the narrower of the two so the
	// rich detail panel gets the bulk of the width.
	homeListPaneMin = 30
	homeListPaneMax = 46
	homeListBiasNum = 38 // ~38% of width to the list column
	homeListBiasDen = 100

	// homeDetailInnerMin is the floor for the detail panel's inner content width so
	// the kv rows stay legible even at the breakpoint.
	homeDetailInnerMin = 26
)

// homeFixedChromeLines is the vertical space (rows) consumed above + below the
// list viewport before the footer: title row, gap, and the panel border.
func homeFixedChromeLines() int {
	return homeTitleLines + homeTitleGapLines + homePanelBorderV
}

// homeFooterHelpLines is the rendered footer height. The home footer is the
// auth/status line + two-tier key hints (buildFooter). It wraps at narrow widths,
// so the budget MUST measure the real footer rather than assume a fixed count.
// homeListViewportHeight takes a measured value to keep View and the math in sync;
// the desync guard test pins measured == lipgloss.Height(m.buildFooter()).

// homeListViewportHeight returns the bubble-list height given the terminal
// height/width and the measured footer height. The list panel border and the
// title chrome are subtracted from the terminal along with the footer.
func homeListViewportHeight(h, w, footerH int) int {
	if h < 1 {
		return 1
	}
	fixed := homeFixedChromeLines()
	avail := h - fixed - footerH
	if avail < 1 {
		return 1
	}
	return avail
}

// homeSplitWidths returns the OUTER widths of the list and detail panels (each
// already includes its border) for a two-column layout at content width w. The
// list is biased narrower (capped homeListPaneMin..homeListPaneMax); the detail
// panel takes the rest after the inter-column gap. Inner content floors are kept
// sane so neither Panel collapses.
func homeSplitWidths(w int) (listPaneW, detailPaneW int) {
	if w < homeSplitMinWidth {
		// Caller shouldn't split below the breakpoint, but stay total-safe.
		w = homeSplitMinWidth
	}

	listPaneW = w * homeListBiasNum / homeListBiasDen
	if listPaneW < homeListPaneMin {
		listPaneW = homeListPaneMin
	}
	if listPaneW > homeListPaneMax {
		listPaneW = homeListPaneMax
	}

	detailPaneW = w - listPaneW - homeColumnGap

	// Keep the detail panel above its inner floor (+4 = border + one space pad each
	// side). If the list bias starves it, reclaim width from the list column.
	minDetailOuter := homeDetailInnerMin + 4
	if detailPaneW < minDetailOuter {
		detailPaneW = minDetailOuter
		listPaneW = w - detailPaneW - homeColumnGap
		if listPaneW < homeListPaneMin {
			listPaneW = homeListPaneMin
			// As a last resort shrink the detail panel to whatever remains; the
			// floors above guarantee w >= homeSplitMinWidth keeps this >= sane.
			detailPaneW = w - listPaneW - homeColumnGap
		}
	}

	return listPaneW, detailPaneW
}
