package ui

import (
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
)

// rpNestedCatalog mirrors the real Vanilla Tweaks shape: a category with its own
// packs AND a subcategory (Aesthetic), and a pure container with no own packs
// (GUI) — the two cases the collapsible tree must handle.
func rpNestedCatalog() *api.ResourcePackCatalog {
	return &api.ResourcePackCatalog{
		Version: "1.21",
		Categories: []api.RPCategory{
			{
				Category: "Aesthetic",
				Packs: []api.RPPack{
					{Name: "BorderlessGlass", Display: "Borderless Glass"},
					{Name: "ClearGlass", Display: "Clear Glass"},
				},
				Categories: []api.RPCategory{
					{
						Category: "More Zombies",
						Packs: []api.RPPack{
							{Name: "MZSteve", Display: "Steve Zombies"},
							{Name: "MZAlex", Display: "Alex Zombies"},
						},
					},
				},
			},
			{
				Category: "GUI",
				Categories: []api.RPCategory{
					{Category: "Fonts", Packs: []api.RPPack{{Name: "F1", Display: "Font One"}}},
					{Category: "Crosshairs", Packs: []api.RPPack{{Name: "C1", Display: "Cross One"}}},
				},
			},
		},
	}
}

func rpTreeModel(t *testing.T) *ResourcePacksModel {
	t.Helper()
	inst := &core.Instance{Path: t.TempDir(), Name: "Tree", Version: "1.21.4"}
	m := NewResourcePacksModel(inst, nil)
	m.catalog = rpNestedCatalog()
	m.catalogSupported = true
	m.catalogVersion = "1.21"
	m.sel = resourcepacks.NewSelection("1.21")
	m.SetSize(100, 40) // triggers rebuildCategoryItems via relabelCategories
	return m
}

func rpVisiblePaths(m *ResourcePacksModel) []string {
	out := make([]string, 0, len(m.flatCategories))
	for _, it := range m.flatCategories {
		out = append(out, it.path)
	}
	return out
}

func rpContains(paths []string, want string) bool {
	for _, p := range paths {
		if p == want {
			return true
		}
	}
	return false
}

// Default state: only top-level categories are visible; children stay hidden
// until their container is expanded (progressive disclosure).
func TestRPTreeCollapsedByDefault(t *testing.T) {
	m := rpTreeModel(t)
	paths := rpVisiblePaths(m)
	if len(paths) != 2 {
		t.Fatalf("expected 2 top-level rows, got %d: %v", len(paths), paths)
	}
	if rpContains(paths, joinPath("Aesthetic", "More Zombies")) {
		t.Errorf("subcategory should be hidden while collapsed: %v", paths)
	}
	if rpContains(paths, joinPath("GUI", "Fonts")) {
		t.Errorf("GUI children should be hidden while collapsed: %v", paths)
	}
}

// Expanding a container reveals its children; collapsing hides them again.
func TestRPTreeExpandCollapse(t *testing.T) {
	m := rpTreeModel(t)
	m.setExpanded("Aesthetic", true)
	if got := rpVisiblePaths(m); !rpContains(got, joinPath("Aesthetic", "More Zombies")) {
		t.Fatalf("expected More Zombies visible after expand: %v", got)
	}
	m.setExpanded("Aesthetic", false)
	if got := rpVisiblePaths(m); rpContains(got, joinPath("Aesthetic", "More Zombies")) {
		t.Fatalf("expected More Zombies hidden after collapse: %v", got)
	}
}

// Aggregate counts include descendants; a pick in a subcategory bumps the
// ancestor's badge even while the ancestor is collapsed.
func TestRPTreeAggregateCounts(t *testing.T) {
	m := rpTreeModel(t)
	// Total packs: Aesthetic 2 own + 2 in More Zombies = 4.
	aes := m.flatCategories[0]
	if aes.category.Category != "Aesthetic" {
		t.Fatalf("expected Aesthetic first, got %q", aes.category.Category)
	}
	if aes.totalPacks != 4 {
		t.Errorf("Aesthetic totalPacks = %d, want 4", aes.totalPacks)
	}
	// Select a pack inside the subcategory.
	m.sel.Toggle(m.catalog, "More Zombies", "MZSteve")
	m.rebuildCategoryItems()
	if got := m.flatCategories[0].selectedCount; got != 1 {
		t.Errorf("Aesthetic aggregate selectedCount = %d, want 1 (collapsed ancestor)", got)
	}
}

// navRight on a collapsed container expands it; a second navRight on a pure
// container (no own packs) steps into its first child rather than an empty pane.
func TestRPTreeNavRightDrillsIn(t *testing.T) {
	m := rpTreeModel(t)
	if !m.selectCategoryByPath("GUI") {
		t.Fatal("GUI row not found")
	}
	m.navRight() // expand GUI
	if !m.expanded["GUI"] {
		t.Fatal("navRight should expand a collapsed container")
	}
	m.navRight() // pure container -> dive to first child
	if got := m.selectedCategoryPath(); got != joinPath("GUI", "Fonts") {
		t.Fatalf("navRight on pure container should select first child, got %q", got)
	}
}

// Auto-expand: opening the screen with a subcategory pick already in the cart
// reveals that branch so the user sees their selection.
func TestRPTreeAutoExpandSelected(t *testing.T) {
	m := rpTreeModel(t)
	m.sel.Toggle(m.catalog, "Fonts", "F1")
	m.autoExpandSelected()
	m.rebuildCategoryItems()
	if got := rpVisiblePaths(m); !rpContains(got, joinPath("GUI", "Fonts")) {
		t.Fatalf("expected GUI auto-expanded to show selected Fonts pick: %v", got)
	}
}

func TestStripHTML(t *testing.T) {
	cases := []struct{ in, want string }{
		{"Retextures <b>Zombies</b> to look like Steve.", "Retextures Zombies to look like Steve."},
		{"A &amp; B &lt;tag&gt;", "A & B <tag>"},
		{"  spaced\n\tout   text  ", "spaced out text"},
		{"<p>line<br>break</p>", "line break"},
		{"", ""},
	}
	for _, c := range cases {
		if got := stripHTML(c.in); got != c.want {
			t.Errorf("stripHTML(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}
