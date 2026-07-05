package ui

import "testing"

// The swatch color is a stable function of the instance ID (same ID → same
// color across sessions) and spreads across the palette for varied IDs.
func TestSwatch(t *testing.T) {
	restoreDark(t)

	// Stability.
	for _, id := range []string{"survival-world", "abc", "fresh-modpack-1"} {
		if swatchColorForID(id) != swatchColorForID(id) {
			t.Fatalf("swatch color for %q is not stable", id)
		}
	}

	// A blank ID yields the neutral fallback, not a palette color.
	if got := InstanceSwatch(""); got == "" {
		t.Fatal("blank-ID swatch should still render a 2-cell gutter")
	}

	// Spread: across a spread of IDs we should hit at least 3 distinct colors.
	seen := map[string]bool{}
	for _, id := range []string{"a", "b", "c", "d", "e", "f", "g", "h", "survival", "creative", "hardcore", "modded"} {
		seen[string(swatchColorForID(id))] = true
	}
	if len(seen) < 3 {
		t.Fatalf("expected the swatch hash to spread across the palette, got %d distinct colors", len(seen))
	}
}
