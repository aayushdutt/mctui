package ui

import "testing"

// Reasonable minimums where both split and stacked layouts fit chrome + micro footer + small lists.
const modsLayoutTestMinH, modsLayoutTestMinW = 36, 40

// Split layout: outer padding + header + gaps + pane border + right chrome + both list viewports + help.
func TestModsLayoutSplitFitsTerminalHeight(t *testing.T) {
	for termH := modsLayoutTestMinH; termH <= 120; termH++ {
		for termW := modsLayoutTestMinW; termW <= 160; termW++ {
			listH := modsSplitListViewportHeight(termH, termW)
			helpH := modsFooterHelpLines(termH, termW)
			total := modsLayoutSplitFixedLines() + listH + helpH
			if total > termH {
				t.Fatalf("split vertical budget exceeded: %dx%d used=%d (listH=%d helpH=%d fixed=%d)",
					termW, termH, total, listH, helpH, modsLayoutSplitFixedLines())
			}
		}
	}
}

// Stacked layout: fixed band + installed shell + browse shell + both list heights.
func TestModsLayoutCompactFitsTerminalHeight(t *testing.T) {
	for termH := modsLayoutTestMinH; termH <= 120; termH++ {
		for termW := modsLayoutTestMinW; termW <= 160; termW++ {
			libH, brH := modsCompactListHeights(termH, termW)
			helpH := modsFooterHelpLinesFromItems(modsCompactFooterItems(termH, termW), termW)
			libPane := modsPanelBorderV + modsLibraryChromeLines
			brPane := modsPanelBorderV + modsRightColumnChromeLines + modsBrowseSectionHdrLines
			fixed := modsOuterPadY + modsHeaderLines + 3 + helpH
			total := fixed + libPane + libH + brPane + brH
			if total > termH {
				t.Fatalf("compact vertical budget exceeded: %dx%d used=%d (libH=%d brH=%d)",
					termW, termH, total, libH, brH)
			}
		}
	}
}

// Help line count must match the width ModsModel.View passes to buildHelpText.
func TestModsHelpBodyMaxWidthMatchesView(t *testing.T) {
	w := 53
	if got, want := modsHelpBodyMaxWidth(w), w-6; got != want {
		t.Fatalf("modsHelpBodyMaxWidth(%d)=%d want %d", w, got, want)
	}
}
