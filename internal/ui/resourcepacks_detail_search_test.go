package ui

import (
	"strings"
	"testing"

	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
)

// Pressing esc while still typing the global-search filter must fully leave
// search — not leave searchMode stuck true with an unfiltered flattened list.
// This drives the real handleKey path (not the helpers) since the bug lived
// in the Filtering-state branch.
func TestRPSearchEscWhileTyping(t *testing.T) {
	m := rpTreeModel(t)
	m.selectCategoryByDisplay("Aesthetic")
	m.rebuildPackItems()
	perCategory := len(m.packs.Items())

	send := func(k tea.KeyMsg) { _, _ = m.Update(k) }
	send(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("/")}) // enter search + start filter
	if !m.searchMode || m.packs.FilterState() != list.Filtering {
		t.Fatalf("expected search+Filtering, got searchMode=%v state=%v", m.searchMode, m.packs.FilterState())
	}
	send(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("z")}) // type a char
	send(tea.KeyMsg{Type: tea.KeyEsc})                       // cancel mid-type

	if m.searchMode {
		t.Error("esc while typing should clear searchMode")
	}
	if m.packs.FilterState() != list.Unfiltered {
		t.Errorf("filter should be cleared, got %v", m.packs.FilterState())
	}
	if got := len(m.packs.Items()); got != perCategory {
		t.Errorf("pack list should rebuild to per-category (%d), got %d (stuck flattened)", perCategory, got)
	}
}

func rpPackIndexByName(m *ResourcePacksModel, name string) int {
	for i, it := range m.packs.Items() {
		if p, ok := it.(rpPackItem); ok && p.pack.Pack.Name == name {
			return i
		}
	}
	return -1
}

// Global search flattens every pack; jumping lands on the pack inside its
// category with ancestors expanded.
func TestRPSearchJump(t *testing.T) {
	m := rpTreeModel(t)
	m.enterSearch()
	if !m.searchMode {
		t.Fatal("enterSearch should set searchMode")
	}
	// All packs flattened: Aesthetic(2) + More Zombies(2) + Fonts(1) + Crosshairs(1) = 6.
	if got := len(m.packs.Items()); got != 6 {
		t.Fatalf("search list = %d packs, want 6 (flattened)", got)
	}
	// Highlight a pack that lives in a nested, collapsed subcategory and jump.
	idx := rpPackIndexByName(m, "MZSteve")
	if idx < 0 {
		t.Fatal("MZSteve not in flattened search list")
	}
	m.packs.Select(idx)
	m.jumpToSearchResult()

	if m.searchMode {
		t.Error("jump should exit search mode")
	}
	if !m.expanded["Aesthetic"] {
		t.Error("jump should expand the ancestor container")
	}
	if cat, _ := m.selectedCategory(); cat.category.Category != "More Zombies" {
		t.Errorf("jump landed on category %q, want More Zombies", cat.category.Category)
	}
	if ref, ok := m.selectedPackRef(); !ok || ref.Pack.Name != "MZSteve" {
		t.Errorf("jump should highlight MZSteve, got %+v (ok=%v)", ref.Pack.Name, ok)
	}
	if m.rpFocus != rpPanelPacks {
		t.Error("jump should focus the packs pane")
	}
}

// Applied-vs-new markers: a built pack renders [x], a newly added one [+], and a
// removed-but-built one [-].
func TestRPAppliedMarkers(t *testing.T) {
	m := rpTreeModel(t)
	// Build ClearGlass into the applied snapshot.
	m.sel.Toggle(m.catalog, "Aesthetic", "ClearGlass")
	m.sel.MarkApplied()
	// Add BorderlessGlass to the cart (not yet applied).
	m.sel.Toggle(m.catalog, "Aesthetic", "BorderlessGlass")
	m.selectCategoryByDisplay("Aesthetic")
	m.rebuildPackItems()

	check := func(name, wantBox string) {
		i := rpPackIndexByName(m, name)
		if i < 0 {
			t.Fatalf("%s not in pack list", name)
		}
		title := m.packs.Items()[i].(rpPackItem).Title()
		if !strings.Contains(title, wantBox) {
			t.Errorf("%s title %q, want box %q", name, title, wantBox)
		}
	}
	check("ClearGlass", "[x]")      // built + in cart
	check("BorderlessGlass", "[+]") // newly added, pending

	// Now remove the applied pack from the cart -> pending removal [-].
	m.sel.Toggle(m.catalog, "Aesthetic", "ClearGlass")
	m.rebuildPackItems()
	check("ClearGlass", "[-]")
}

// The highlighted pack's full description renders in the wide status line (the
// replacement for the dropped details modal), and enter toggles the pack.
func TestRPHighlightDescriptionAndEnterToggles(t *testing.T) {
	m := rpTreeModel(t)
	m.catalog.Categories[0].Packs[1].Description = "Makes glass <b>fully clear</b> with no border."
	m.selectCategoryByDisplay("Aesthetic")
	m.rebuildPackItems()
	m.rpFocus = rpPanelPacks
	m.packs.Select(rpPackIndexByName(m, "ClearGlass"))

	if line := m.statusLine(); !strings.Contains(line, "Makes glass") {
		t.Errorf("status line should show the highlighted pack description, got %q", line)
	}
	// enter toggles (no modal): the pack lands in the cart.
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if !m.sel.Has("Aesthetic", "ClearGlass") {
		t.Error("enter should toggle the highlighted pack into the cart")
	}
}
