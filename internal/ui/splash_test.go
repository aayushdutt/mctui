package ui

import (
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

// The process splash is chosen once and stable for the lifetime of the binary,
// is drawn from the curated pool, and stays within the ≤40-cell budget.
func TestSplash(t *testing.T) {
	restoreDark(t)
	if appSplash == "" {
		t.Fatal("appSplash should be a non-empty curated line")
	}
	found := false
	for _, s := range splashLines {
		if s == appSplash {
			found = true
		}
		if lipgloss.Width(s) > 40 {
			t.Errorf("splash line exceeds 40 cells: %q (%d)", s, lipgloss.Width(s))
		}
		if strings.TrimSpace(s) == "" {
			t.Error("empty splash line in the pool")
		}
	}
	if !found {
		t.Fatalf("appSplash %q is not from the curated pool", appSplash)
	}

	// Stable across calls (picked once per process).
	if SplashLine() == "" {
		t.Fatal("SplashLine should render the chosen splash")
	}
	if !strings.Contains(SplashLine(), appSplash) {
		t.Fatal("SplashLine should contain the chosen splash text")
	}
}
