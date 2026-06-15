package resourcepacks

import (
	"reflect"
	"sort"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

// buildCatalog constructs a one-category catalog from (name, incompatible...) packs.
func buildCatalog(category string, packs ...api.RPPack) *api.ResourcePackCatalog {
	return &api.ResourcePackCatalog{
		Version: "1.21",
		Categories: []api.RPCategory{
			{Category: category, Packs: packs},
		},
	}
}

func TestToggleIncompatibilityResolution(t *testing.T) {
	// Catalog where A is incompatible-with B (one-directional, declared on A),
	// C declares incompatibility with D (so adding D must evict C — incoming edge),
	// and E<->F are mutually declared.
	cat := buildCatalog("Aesthetic",
		api.RPPack{Name: "A", Incompatible: []string{"B"}},
		api.RPPack{Name: "B"},
		api.RPPack{Name: "C", Incompatible: []string{"D"}},
		api.RPPack{Name: "D"},
		api.RPPack{Name: "E", Incompatible: []string{"F"}},
		api.RPPack{Name: "F", Incompatible: []string{"E"}},
		api.RPPack{Name: "G"},
	)

	tests := []struct {
		name        string
		preselect   [][2]string // (category, name) already in cart
		toggleCat   string
		toggleName  string
		wantEnabled bool
		wantRemoved []string
		wantAfter   []string // pack ids remaining in cart, sorted
	}{
		{
			name:        "add to empty cart, no conflicts",
			toggleCat:   "Aesthetic",
			toggleName:  "G",
			wantEnabled: true,
			wantRemoved: nil,
			wantAfter:   []string{"G"},
		},
		{
			name:        "outgoing edge evicts: add A removes B",
			preselect:   [][2]string{{"Aesthetic", "B"}},
			toggleCat:   "Aesthetic",
			toggleName:  "A",
			wantEnabled: true,
			wantRemoved: []string{"B"},
			wantAfter:   []string{"A"},
		},
		{
			name:        "incoming edge evicts: add D removes C (C declares D)",
			preselect:   [][2]string{{"Aesthetic", "C"}},
			toggleCat:   "Aesthetic",
			toggleName:  "D",
			wantEnabled: true,
			wantRemoved: []string{"C"},
			wantAfter:   []string{"D"},
		},
		{
			name:        "bidirectional: add F removes E",
			preselect:   [][2]string{{"Aesthetic", "E"}},
			toggleCat:   "Aesthetic",
			toggleName:  "F",
			wantEnabled: true,
			wantRemoved: []string{"E"},
			wantAfter:   []string{"F"},
		},
		{
			name:        "no conflict when incompatible pack absent",
			preselect:   [][2]string{{"Aesthetic", "G"}},
			toggleCat:   "Aesthetic",
			toggleName:  "A",
			wantEnabled: true,
			wantRemoved: nil,
			wantAfter:   []string{"A", "G"},
		},
		{
			name:        "toggle off removes pack, no eviction reported",
			preselect:   [][2]string{{"Aesthetic", "A"}, {"Aesthetic", "G"}},
			toggleCat:   "Aesthetic",
			toggleName:  "A",
			wantEnabled: false,
			wantRemoved: nil,
			wantAfter:   []string{"G"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			sel := NewSelection("1.21")
			for _, p := range tt.preselect {
				sel.add(p[0], p[1])
			}
			res := sel.Toggle(cat, tt.toggleCat, tt.toggleName)

			if res.Enabled != tt.wantEnabled {
				t.Errorf("Enabled = %v, want %v", res.Enabled, tt.wantEnabled)
			}
			if !reflect.DeepEqual(res.Removed, tt.wantRemoved) {
				t.Errorf("Removed = %v, want %v", res.Removed, tt.wantRemoved)
			}

			var got []string
			for _, names := range sel.Packs {
				got = append(got, names...)
			}
			sort.Strings(got)
			if !reflect.DeepEqual(got, tt.wantAfter) {
				t.Errorf("cart after = %v, want %v", got, tt.wantAfter)
			}
		})
	}
}

// TestToggleChainEviction verifies a single toggle evicts multiple conflicting
// packs across categories at once (a "chain" of incoming + outgoing edges).
func TestToggleChainEviction(t *testing.T) {
	// X is incompatible with Y (outgoing). Z declares X (incoming). Adding X
	// should evict both Y and Z in one toggle.
	cat := &api.ResourcePackCatalog{
		Version: "1.21",
		Categories: []api.RPCategory{
			{Category: "Aesthetic", Packs: []api.RPPack{
				{Name: "X", Incompatible: []string{"Y"}},
				{Name: "Y"},
			}},
			{Category: "Terrain", Packs: []api.RPPack{
				{Name: "Z", Incompatible: []string{"X"}},
			}},
		},
	}

	sel := NewSelection("1.21")
	sel.add("Aesthetic", "Y")
	sel.add("Terrain", "Z")

	res := sel.Toggle(cat, "Aesthetic", "X")
	if !res.Enabled {
		t.Fatalf("expected X enabled")
	}
	want := []string{"Y", "Z"}
	if !reflect.DeepEqual(res.Removed, want) {
		t.Fatalf("Removed = %v, want %v", res.Removed, want)
	}
	if sel.Has("Aesthetic", "Y") || sel.Has("Terrain", "Z") {
		t.Fatalf("conflicting packs not evicted: %#v", sel.Packs)
	}
	if !sel.Has("Aesthetic", "X") {
		t.Fatalf("X not added")
	}
	// Empty category should be pruned.
	if _, ok := sel.Packs["Terrain"]; ok {
		t.Fatalf("emptied Terrain category should be pruned: %#v", sel.Packs)
	}
}

// TestToggleNestedCatalogIncompat ensures incompatibility resolution sees packs
// in nested subcategories.
func TestToggleNestedCatalogIncompat(t *testing.T) {
	cat := &api.ResourcePackCatalog{
		Categories: []api.RPCategory{
			{
				Category: "Aesthetic",
				Categories: []api.RPCategory{
					{Category: "Glass", Packs: []api.RPPack{
						{Name: "ClearGlass", Incompatible: []string{"BorderlessGlass"}},
						{Name: "BorderlessGlass"},
					}},
				},
			},
		},
	}
	sel := NewSelection("1.21")
	sel.add("Aesthetic", "BorderlessGlass")
	res := sel.Toggle(cat, "Aesthetic", "ClearGlass")
	if !reflect.DeepEqual(res.Removed, []string{"BorderlessGlass"}) {
		t.Fatalf("nested incompat not resolved, Removed = %v", res.Removed)
	}
}

func TestCountHasDirty(t *testing.T) {
	sel := NewSelection("1.21")
	if sel.Count() != 0 || sel.Dirty() {
		t.Fatalf("fresh selection should be empty and not dirty")
	}
	sel.add("Aesthetic", "A")
	sel.add("Aesthetic", "B")
	sel.add("Terrain", "C")

	if got := sel.Count(); got != 3 {
		t.Errorf("Count = %d, want 3", got)
	}
	if got := sel.CountInCategory("Aesthetic"); got != 2 {
		t.Errorf("CountInCategory = %d, want 2", got)
	}
	if !sel.Has("Terrain", "C") {
		t.Errorf("Has(Terrain,C) = false, want true")
	}
	if !sel.Dirty() {
		t.Errorf("selection with packs and no Applied should be dirty")
	}

	sel.MarkApplied()
	if sel.Dirty() {
		t.Errorf("after MarkApplied should not be dirty")
	}

	// Reordering within a category should not register as dirty.
	sel.Packs["Aesthetic"] = []string{"B", "A"}
	if sel.Dirty() {
		t.Errorf("reorder within category should not be dirty")
	}

	// Adding a pack makes it dirty again.
	sel.add("Terrain", "D")
	if !sel.Dirty() {
		t.Errorf("after adding pack should be dirty")
	}
}

func TestBuildSelectionMapSortedNonEmpty(t *testing.T) {
	sel := NewSelection("1.21")
	sel.add("Aesthetic", "ClearGlass")
	sel.add("Aesthetic", "BorderlessGlass")
	sel.Packs["Empty"] = []string{} // should be omitted

	m := sel.BuildSelectionMap()
	if _, ok := m["Empty"]; ok {
		t.Errorf("empty category should be omitted")
	}
	want := []string{"BorderlessGlass", "ClearGlass"}
	if !reflect.DeepEqual(m["Aesthetic"], want) {
		t.Errorf("Aesthetic = %v, want sorted %v", m["Aesthetic"], want)
	}
}

func TestLoadSelectionMissingAndCorrupt(t *testing.T) {
	dir := t.TempDir()
	inst := &core.Instance{Path: dir}

	// Missing file -> empty selection, no error.
	sel, err := LoadSelection(inst)
	if err != nil {
		t.Fatalf("LoadSelection missing: %v", err)
	}
	if sel.Count() != 0 {
		t.Fatalf("missing cart should be empty")
	}

	// Round-trip save/load.
	sel.Version = "1.21"
	sel.add("Aesthetic", "A")
	if err := SaveSelection(inst, sel); err != nil {
		t.Fatalf("SaveSelection: %v", err)
	}
	got, err := LoadSelection(inst)
	if err != nil {
		t.Fatalf("LoadSelection roundtrip: %v", err)
	}
	if !got.Has("Aesthetic", "A") || got.Version != "1.21" {
		t.Fatalf("roundtrip mismatch: %#v", got)
	}

	// Corrupt file -> empty selection, no error.
	if err := atomicWriteFile(cartPath(inst), []byte("{not json")); err != nil {
		t.Fatalf("write corrupt: %v", err)
	}
	got, err = LoadSelection(inst)
	if err != nil {
		t.Fatalf("LoadSelection corrupt should not error: %v", err)
	}
	if got.Count() != 0 {
		t.Fatalf("corrupt cart should yield empty selection")
	}
}

// TestSelectionCloneIsDeep verifies Clone produces an independent copy: mutating
// the clone (as BuildAndApply does via MarkApplied) must not touch the original.
// This is the invariant the apply path relies on to avoid racing the UI render
// loop, which reads the live Selection while a copy is handed to the goroutine.
func TestSelectionCloneIsDeep(t *testing.T) {
	orig := NewSelection("1.21")
	orig.add("Aesthetic", "ClearGlass")
	orig.add("Aesthetic", "BorderlessGlass")
	orig.add("Terrain", "BlueNetherrack")

	clone := orig.Clone()
	if clone == orig {
		t.Fatalf("Clone returned the same pointer")
	}
	if !selectionsEqual(orig.Packs, clone.Packs) {
		t.Fatalf("clone Packs differ from original")
	}
	if clone.Version != orig.Version {
		t.Fatalf("clone Version = %q, want %q", clone.Version, orig.Version)
	}

	// Mutating the clone (MarkApplied snapshots Packs into Applied) must not affect
	// the original's Applied or Packs.
	clone.MarkApplied()
	if orig.Applied != nil {
		t.Fatalf("mutating clone.MarkApplied touched orig.Applied: %v", orig.Applied)
	}

	// Mutating the clone's Packs must not alias the original's slices.
	clone.add("Aesthetic", "LowerShield")
	if orig.Has("Aesthetic", "LowerShield") {
		t.Fatalf("clone.add leaked into original (shared slice)")
	}

	// And the reverse: mutating the original must not affect the clone.
	orig.Remove("Terrain", "BlueNetherrack")
	if !clone.Has("Terrain", "BlueNetherrack") {
		t.Fatalf("original.Remove affected the clone")
	}
}

// TestSelectionCloneNil guards the nil receiver.
func TestSelectionCloneNil(t *testing.T) {
	var s *Selection
	if s.Clone() != nil {
		t.Fatalf("nil.Clone() should be nil")
	}
}
