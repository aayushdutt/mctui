package ui

import (
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
)

// rpTestCatalog returns a small catalog with a warning-bearing category so View
// exercises both the cart CTA block and the category warning line — the two
// blocks the abstract budget test (resourcepacks_layout_test.go) does not see.
func rpTestCatalog() *api.ResourcePackCatalog {
	return &api.ResourcePackCatalog{
		Version: "1.21",
		Categories: []api.RPCategory{
			{
				Category: "Aesthetic",
				Warning:  &api.RPWarning{Text: "Some packs here are experimental.", Color: "red"},
				Packs: []api.RPPack{
					{Name: "BorderlessGlass", Display: "Borderless Glass"},
					{Name: "ClearGlass", Display: "Clear Glass"},
					{Name: "LowerShield", Display: "Lower Shield"},
				},
			},
			{
				Category: "Terrain",
				Packs: []api.RPPack{
					{Name: "BlueNetherrack", Display: "Blue Netherrack"},
					{Name: "NaturalGravel", Display: "Natural Gravel"},
				},
			},
		},
	}
}

// rpRenderModel builds a fully-populated, sized resource-packs model ready to
// View, with the given cart contents (category -> pack ids).
func rpRenderModel(t *testing.T, w, h int, cart map[string][]string) *ResourcePacksModel {
	t.Helper()
	inst := &core.Instance{Name: "Test", Version: "1.21.4"}
	m := NewResourcePacksModel(inst, nil)
	m.catalog = rpTestCatalog()
	m.catalogSupported = true
	m.catalogVersion = "1.21"

	sel := resourcepacks.NewSelection("1.21")
	for cat, names := range cart {
		for _, n := range names {
			sel.Toggle(m.catalog, cat, n)
		}
	}
	m.sel = sel

	m.rebuildCategoryItems()
	m.rebuildPackItems()
	m.rebuildCartItems()
	m.SetSize(w, h)
	return m
}

// rpViewLines is the number of rows View renders at w×h with the given cart.
func rpViewLines(t *testing.T, w, h int, cart map[string][]string) (int, string) {
	t.Helper()
	out := rpRenderModel(t, w, h, cart).View()
	return strings.Count(out, "\n") + 1, out
}

// TestRPViewFitsTerminalHeight renders the ACTUAL View output (not just the
// abstract budget) and asserts it never exceeds the terminal height — including
// the non-empty-cart case (which appends the "Build & Apply" CTA) and the
// category-warning case (the default category carries one, prepending a line).
// This is the regression guard for the cart-CTA / warning height overflow that
// the abstract budget test cannot see.
//
// Sizes are the supported split layout (w≥rpSplitMinWidth) across its height
// range. The stacked/compact layout is exercised separately at heights where it
// fits: three bubbles lists each render a ~7-row minimum, so the stacked layout
// has an inherent floor (~49 rows) that is independent of this fix.
func TestRPViewFitsTerminalHeight(t *testing.T) {
	sizes := []struct{ w, h int }{
		{78, 24}, {78, 42}, {88, 26}, {88, 60}, {100, 40}, {120, 50}, {160, 30},
	}
	carts := map[string]map[string][]string{
		"empty":     {},
		"non-empty": {"Aesthetic": {"ClearGlass", "BorderlessGlass"}},
		"full":      {"Aesthetic": {"ClearGlass", "BorderlessGlass", "LowerShield"}},
	}

	for name, cart := range carts {
		for _, s := range sizes {
			name, s, cart := name, s, cart
			t.Run(name, func(t *testing.T) {
				got, out := rpViewLines(t, s.w, s.h, cart)
				if got > s.h {
					t.Fatalf("%s cart: View at %dx%d rendered %d lines (> %d)\n---\n%s",
						name, s.w, s.h, got, s.h, out)
				}
			})
		}
	}
}

// TestRPViewCartCTADoesNotOverflow is the direct guard for the reported bug:
// a non-empty cart appends the "[b] Build & Apply" block, which must not push the
// split view past the terminal height. Asserts the full-cart render equals the
// empty-cart render (the CTA budget is reclaimed from the cart list, not added on
// top) at the exact sizes the adversarial review reproduced.
func TestRPViewCartCTADoesNotOverflow(t *testing.T) {
	for _, s := range []struct{ w, h int }{{88, 26}, {100, 40}, {120, 50}} {
		empty, _ := rpViewLines(t, s.w, s.h, map[string][]string{})
		full, out := rpViewLines(t, s.w, s.h, map[string][]string{
			"Aesthetic": {"ClearGlass", "BorderlessGlass", "LowerShield"},
		})
		if full > s.h {
			t.Fatalf("full-cart View at %dx%d rendered %d lines (> %d)\n---\n%s",
				s.w, s.h, full, s.h, out)
		}
		if full != empty {
			t.Errorf("at %dx%d full-cart render (%d) differs from empty (%d): CTA not budgeted",
				s.w, s.h, full, empty)
		}
	}
}

// TestRPViewWarningCategoryFits exercises the warning-bearing category (the
// default-selected first category carries one) together with a full cart — the
// worst case where the warning line and CTA block both apply — in the split
// layout. The warning is truncated to a single budgeted row.
func TestRPViewWarningCategoryFits(t *testing.T) {
	for _, s := range []struct{ w, h int }{{78, 24}, {88, 26}, {100, 40}, {120, 50}} {
		m := rpRenderModel(t, s.w, s.h, map[string][]string{
			"Aesthetic": {"ClearGlass", "BorderlessGlass", "LowerShield"},
		})
		// Sanity: the highlighted category is the warning one.
		if w := m.selectedCategoryWarning(); w == "" {
			t.Fatalf("expected the default-selected category to carry a warning")
		}
		out := m.View()
		if got := strings.Count(out, "\n") + 1; got > s.h {
			t.Fatalf("warning+cart View at %dx%d rendered %d lines (> %d)\n---\n%s",
				s.w, s.h, got, s.h, out)
		}
	}
}
