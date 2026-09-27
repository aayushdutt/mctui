package ui

import (
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

// TestCardGeometry checks a card renders as a solid rectangle: every line is
// exactly `width` visible cells, including the title and wrapped body lines.
func TestCardGeometry(t *testing.T) {
	restoreDark(t)

	cases := []struct {
		name  string
		title string
		body  string
		width int
	}{
		{"titled", "New Instance", "▸ Version   1.21.4\n  Loader    Fabric", 40},
		{"untitled", "", "hello", 20},
		{"long title clamps", "A very very long card title that overflows", "x", 24},
		{"body wraps", "Logs", strings.Repeat("word ", 40), 30},
		{"styled body", "Status", lipgloss.NewStyle().Foreground(Active.Success).Render("● running"), 28},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			out := Card(c.title, c.body, c.width, Active.Primary)
			for i, line := range strings.Split(out, "\n") {
				if w := lipgloss.Width(line); w != c.width {
					t.Errorf("line %d width = %d, want %d\n  %q", i, w, c.width, line)
				}
			}
		})
	}
}

// TestRuleWidth checks the rule is exactly the requested width and empty when
// nonpositive.
func TestRuleWidth(t *testing.T) {
	restoreDark(t)
	if got := lipgloss.Width(Rule(20)); got != 20 {
		t.Errorf("Rule(20) width = %d, want 20", got)
	}
	if Rule(0) != "" {
		t.Error("Rule(0) should be empty")
	}
}

// TestKeyHintsWrap checks the hint bar stays within width and renders each key.
func TestKeyHintsWrap(t *testing.T) {
	restoreDark(t)
	hints := []KeyHint{{"↵", "launch"}, {"n", "new"}, {"d", "delete"}, {"q", "quit"}}
	out := KeyHints(24, hints...)
	for _, line := range strings.Split(out, "\n") {
		if w := lipgloss.Width(line); w > 24 {
			t.Errorf("hint line width %d > 24: %q", w, line)
		}
	}
	for _, h := range hints {
		if !strings.Contains(out, h.Label) {
			t.Errorf("KeyHints output missing label %q", h.Label)
		}
	}
}

func TestClampHeightKeepsTheTop(t *testing.T) {
	if got := ClampHeight("a\nb\nc\nd", 2); got != "a\nb" {
		t.Errorf("ClampHeight = %q, want the first two lines", got)
	}
	if got := ClampHeight("a\nb", 5); got != "a\nb" {
		t.Errorf("a short view must be unchanged, got %q", got)
	}
}
