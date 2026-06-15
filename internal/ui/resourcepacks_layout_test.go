package ui

import "testing"

// Reasonable minimums where both split and stacked layouts fit chrome + footer +
// small lists. Mirrors the mods layout test bounds.
const rpLayoutTestMinH, rpLayoutTestMinW = 36, 40

// Split layout: the per-pane list viewport plus fixed chrome and the footer must
// never exceed the terminal height.
func TestRPLayoutSplitFitsTerminalHeight(t *testing.T) {
	for termH := rpLayoutTestMinH; termH <= 120; termH++ {
		for termW := rpLayoutTestMinW; termW <= 160; termW++ {
			listH := rpListViewportHeight(termH, termW)
			helpH := rpFooterHelpLines(termH, termW)
			fixed := rpOuterPadY + rpHeaderLines + rpPanelBorderV + rpInterBlockGaps + rpStatusLines
			total := fixed + listH + helpH
			if total > termH {
				t.Fatalf("split vertical budget exceeded: %dx%d used=%d (listH=%d helpH=%d fixed=%d)",
					termW, termH, total, listH, helpH, fixed)
			}
		}
	}
}

// Stacked layout: all three list heights plus their shells, gaps, header, status,
// footer, AND the worst-case in-panel chrome (cart CTA + category warning) must
// fit. extraReserve = rpCartCTAReserve + 1 (warning line) is the max overhead
// applyPaneListHeights folds in.
func TestRPLayoutCompactFitsTerminalHeight(t *testing.T) {
	const extraReserve = rpCartCTAReserve + 1
	for termH := rpLayoutTestMinH; termH <= 120; termH++ {
		for termW := rpLayoutTestMinW; termW <= 160; termW++ {
			catH, packH, cartH := rpCompactListHeights(termH, termW, extraReserve)
			helpH := rpFooterHelpLines(termH, termW)
			shells := rpPanelBorderV * 3
			gaps := 4
			pre := rpOuterPadY + rpHeaderLines + rpStatusLines + helpH + shells + gaps + extraReserve
			total := pre + catH + packH + cartH
			if total > termH {
				t.Fatalf("compact vertical budget exceeded: %dx%d used=%d (cat=%d pack=%d cart=%d)",
					termW, termH, total, catH, packH, cartH)
			}
		}
	}
}

// Help line count must match the width ResourcePacksModel.View passes to rpRenderHelp.
func TestRPHelpBodyMaxWidthMatchesView(t *testing.T) {
	w := 53
	if got, want := rpHelpBodyMaxWidth(w), w-6; got != want {
		t.Fatalf("rpHelpBodyMaxWidth(%d)=%d want %d", w, got, want)
	}
}
