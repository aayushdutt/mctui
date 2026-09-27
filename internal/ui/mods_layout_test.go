package ui

import (
	"fmt"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/lipgloss"
)

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
			helpH := modsFooterHelpLines(termH, termW)
			libPane := modsPanelBorderV + modsLibraryChromeLines
			brPane := modsPanelBorderV + modsRightColumnChromeLines + modsBrowseSectionHdrLines
			fixed := modsOuterPadY + modsHeaderLines + 2 + helpH
			total := fixed + libPane + libH + brPane + brH
			if total > termH {
				t.Fatalf("compact vertical budget exceeded: %dx%d used=%d (libH=%d brH=%d)",
					termW, termH, total, libH, brH)
			}
		}
	}
}

// Help line count must match the width ModsModel.View passes to modsRenderHelp.
func TestModsHelpBodyMaxWidthMatchesView(t *testing.T) {
	w := 53
	if got, want := modsHelpBodyMaxWidth(w), w; got != want {
		t.Fatalf("modsHelpBodyMaxWidth(%d)=%d want %d", w, got, want)
	}
}

// The real guard: render the mods screen at many sizes (split and stacked, each
// pane focused) and check it never draws more rows than the terminal has —
// bubbletea would drop the top rows (the header) of a taller view.
func TestModsViewFitsTerminalHeight(t *testing.T) {
	restoreDark(t)
	inst := fabricInstance(t)[0]
	// Enough rows in both lists to page, so each list draws its page dots.
	var hits []api.SearchHit
	for i := range 12 {
		mustWrite(t, fmt.Sprintf("%s/.minecraft/mods/mod%d-1.0.jar", inst.Path, i), 10)
		hits = append(hits, api.SearchHit{ProjectID: fmt.Sprint(i), Title: fmt.Sprintf("Mod %d", i), Downloads: 1000})
	}
	// And an instance with no mods, which shows a wrapped hint in place of the
	// installed list.
	empty := &core.Instance{ID: "e", Name: "Empty", Version: "1.21.4", Loader: "fabric", Path: t.TempDir()}
	for _, in := range []*core.Instance{inst, empty} {
		for _, w := range []int{60, 76, 96, 140} {
			for h := 20; h <= 48; h += 3 {
				for _, f := range []modsPanel{panelInstalled, panelQuery, panelBrowse} {
					m := NewModsModel(in, nil)
					m.SetSize(w, h)
					m.results.SetItems(m.hitsToItems(hits))
					m.lastTotalHits = 5000
					m.modsFocus = f
					if got := lipgloss.Height(m.View()); got > h {
						t.Errorf("%s %dx%d focus=%d: view is %d rows", in.Name, w, h, f, got)
					}
				}
			}
		}
	}
}
