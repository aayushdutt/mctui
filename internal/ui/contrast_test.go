package ui

import (
	"testing"

	"github.com/charmbracelet/lipgloss"
)

// TestOnColorFallsBackOnNonHex checks that colors OnColor can't parse (ANSI
// indices, empty, short hex) fall back to light text rather than panicking or
// returning an empty foreground.
func TestOnColorFallsBackOnNonHex(t *testing.T) {
	restoreDark(t)
	for _, c := range []lipgloss.Color{
		lipgloss.Color("205"),  // ANSI 256 index
		lipgloss.Color(""),     // empty
		lipgloss.Color("#abc"), // 3-digit shorthand (unsupported by hexRGB)
	} {
		if got := OnColor(c); got != onLightText {
			t.Errorf("OnColor(%q) = %q, want fallback %q", string(c), got, onLightText)
		}
	}
}

// TestContrastRatio checks the ratio for known pairs and that unparseable colors
// report ok=false instead of a bogus number.
func TestContrastRatio(t *testing.T) {
	restoreDark(t)

	r, ok := ContrastRatio(lipgloss.Color("#FFFFFF"), lipgloss.Color("#000000"))
	if !ok {
		t.Fatal("ContrastRatio(white, black) ok = false, want true")
	}
	if r < 20.5 || r > 21.5 {
		t.Errorf("ContrastRatio(white, black) = %.2f, want ~21", r)
	}

	if _, ok := ContrastRatio(lipgloss.Color("205"), lipgloss.Color("#000000")); ok {
		t.Error("ContrastRatio with an ANSI color reported ok=true, want false")
	}
}
