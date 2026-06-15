package resourcepacks

import (
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
)

func blockerCatalog() *api.ResourcePackCatalog {
	return &api.ResourcePackCatalog{Categories: []api.RPCategory{{
		Category: "Aesthetic",
		Packs: []api.RPPack{
			{Name: "A", Display: "A", Incompatible: []string{"B"}}, // A declares conflict with B
			{Name: "B", Display: "B"},                              // B declares nothing (reverse edge)
			{Name: "C", Display: "C"},                              // unrelated
		},
	}}}
}

func TestBlockerFor(t *testing.T) {
	cat := blockerCatalog()
	s := NewSelection("1.21")
	s.add("Aesthetic", "A") // A is in the cart

	// B conflicts with A by the reverse edge (A lists B) — blocked by A.
	if id, ok := s.BlockerFor(cat, "B"); !ok || id != "A" {
		t.Errorf("BlockerFor(B) = (%q,%v), want (A,true)", id, ok)
	}
	// C has no conflict — not blocked.
	if _, ok := s.BlockerFor(cat, "C"); ok {
		t.Errorf("BlockerFor(C) should be unblocked")
	}
	// A itself is already in the cart — never reports as blocked.
	if _, ok := s.BlockerFor(cat, "A"); ok {
		t.Errorf("BlockerFor(A) should be false (already selected)")
	}
}
