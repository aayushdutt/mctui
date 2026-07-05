package ui

import (
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/lipgloss"
)

// Reasonable minimums where the two-column home layout fits chrome + footer +
// a small list. Mirrors the resource-packs layout test bounds.
const homeLayoutTestMinH, homeLayoutTestMinW = 24, 84

// Two-column vertical budget: the list viewport plus the fixed chrome (title row,
// gap, panel border) and the measured footer must never exceed the terminal
// height. The footer is measured through a real HomeModel so the help-wrapping
// height matches what View draws.
func TestHomeLayoutSplitFitsTerminalHeight(t *testing.T) {
	for termH := homeLayoutTestMinH; termH <= 120; termH++ {
		for termW := homeLayoutTestMinW; termW <= 160; termW++ {
			m := NewHomeModel()
			m.SetSize(termW, termH)
			footerH := m.homeFooterHeight()
			listH := homeListViewportHeight(termH, termW, footerH)
			total := homeFixedChromeLines() + listH + footerH
			if total > termH {
				t.Fatalf("split vertical budget exceeded: %dx%d used=%d (listH=%d footerH=%d fixed=%d)",
					termW, termH, total, listH, footerH, homeFixedChromeLines())
			}
		}
	}
}

// Two-column widths must fit: list + gap + detail <= terminal width, with sane
// inner floors on both panels, for every width at/above the breakpoint.
func TestHomeSplitWidthsFit(t *testing.T) {
	for termW := homeSplitMinWidth; termW <= 200; termW++ {
		listPaneW, detailPaneW := homeSplitWidths(termW)
		if listPaneW+detailPaneW+homeColumnGap > termW {
			t.Fatalf("widths overflow at w=%d: list=%d detail=%d gap=%d",
				termW, listPaneW, detailPaneW, homeColumnGap)
		}
		// Inner floors: each panel reserves 4 cells for border + padding.
		if listInner := listPaneW - 4; listInner < 1 {
			t.Fatalf("list inner floor broken at w=%d: listPaneW=%d", termW, listPaneW)
		}
		if detailInner := detailPaneW - 4; detailInner < homeDetailInnerMin {
			t.Fatalf("detail inner floor broken at w=%d: detailPaneW=%d inner=%d (min %d)",
				termW, detailPaneW, detailInner, homeDetailInnerMin)
		}
	}
}

// The compact toggle flips at exactly homeSplitMinWidth.
func TestHomeCompactToggle(t *testing.T) {
	const h = 40
	m := NewHomeModel()

	m.SetSize(homeSplitMinWidth-1, h)
	if !m.compactLayout {
		t.Fatalf("expected compactLayout at w=%d", homeSplitMinWidth-1)
	}
	m.SetSize(homeSplitMinWidth, h)
	if m.compactLayout {
		t.Fatalf("expected two-column layout at w=%d", homeSplitMinWidth)
	}
}

// Render-based envelope check: the REAL View() output (not just the budget math)
// must never be wider than the terminal, at any width including below the
// breakpoint. This catches overflow the arithmetic-vs-arithmetic budget tests
// structurally cannot. Height is checked within the supported range (the
// underlying list bubble has an internal minimum height shared by all screens).
func TestHomeViewFitsTerminal(t *testing.T) {
	m := NewHomeModel()
	m.SetInstances([]*core.Instance{
		{ID: "a", Name: "Survival World", Version: "1.21.4", Loader: "fabric", LoaderVer: "0.16.10", Path: t.TempDir()},
		{ID: "b", Name: "Creative Flats", Version: "1.21", Loader: "vanilla", Path: t.TempDir()},
		{ID: "c", Name: "A Very Long Instance Name That Should Truncate Cleanly", Version: "1.20.1", Loader: "fabric", Path: t.TempDir()},
	}, "")

	for _, w := range []int{20, 32, 40, 60, 72, 84, 100, 140, 200} {
		for _, h := range []int{homeLayoutTestMinH, 30, 50} {
			// Every facet tab must stay within bounds, not just the default one —
			// and in BOTH pane-focus states (the detail focus accents the pane
			// title, which could affect widths).
			for tab := homeTab(0); tab < homeTabCount; tab++ {
				for _, focus := range []paneFocus{focusList, focusDetail} {
					m.activeTab = tab
					m.paneFocus = focus
					m.SetSize(w, h)
					v := m.View()
					if got := lipgloss.Width(v); got > w {
						t.Errorf("View(%dx%d) tab=%d focus=%d renders width %d > terminal width %d", w, h, tab, focus, got, w)
					}
					if got := lipgloss.Height(v); got > h {
						t.Errorf("View(%dx%d) tab=%d focus=%d renders height %d > terminal height %d", w, h, tab, focus, got, h)
					}
				}
			}
		}
	}
}

// The actions popup and command palette overlays must also stay within the
// terminal bounds at every width/height, like the base view.
func TestHomeOverlaysFitTerminal(t *testing.T) {
	build := func() *HomeModel {
		m := NewHomeModel()
		m.SetInstances([]*core.Instance{
			{ID: "a", Name: "Survival World", Version: "1.21.4", Loader: "fabric", Path: t.TempDir()},
			{ID: "b", Name: "A Very Long Instance Name That Should Truncate Cleanly", Version: "1.20.1", Loader: "fabric", Path: t.TempDir()},
		}, "a")
		return m
	}
	for _, w := range []int{20, 32, 40, 60, 84, 100, 140, 200} {
		for _, h := range []int{homeLayoutTestMinH, 30, 50} {
			// Actions popup open.
			ma := build()
			ma.SetSize(w, h)
			ma.openActionsPopup(ma.SelectedInstance())
			va := ma.View()
			if got := lipgloss.Width(va); got > w {
				t.Errorf("actions popup View(%dx%d) width %d > %d", w, h, got, w)
			}
			if got := lipgloss.Height(va); got > h {
				t.Errorf("actions popup View(%dx%d) height %d > %d", w, h, got, h)
			}

			// Palette open, with a query typed.
			mp := build()
			mp.SetSize(w, h)
			mp.openPalette()
			mp.palette.input.SetValue("a")
			mp.palette.refilter()
			vp := mp.View()
			if got := lipgloss.Width(vp); got > w {
				t.Errorf("palette View(%dx%d) width %d > %d", w, h, got, w)
			}
			if got := lipgloss.Height(vp); got > h {
				t.Errorf("palette View(%dx%d) height %d > %d", w, h, got, h)
			}
		}
	}
}

// Desync guard: the footer height the budget uses must equal the rendered footer
// height for the same model, across narrow widths where the help wraps.
func TestHomeFooterHeightMatchesView(t *testing.T) {
	for _, w := range []int{40, 52, 60, 72, 84, 100, 140} {
		m := NewHomeModel()
		m.SetSize(w, 40)
		if got, want := m.homeFooterHeight(), lipgloss.Height(m.buildFooter()); got != want {
			t.Fatalf("homeFooterHeight(w=%d)=%d, lipgloss.Height(buildFooter)=%d", w, got, want)
		}
	}
}
