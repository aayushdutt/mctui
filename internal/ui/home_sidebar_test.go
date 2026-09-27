package ui

import (
	"strings"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

// The sidebar marks the selected instance with a thin › pointer before its name
// (a single glyph on one line, so nothing can break into dashes in terminals
// with line spacing), and every row's text starts in the same column.
func TestHome_SidebarRowsAlign(t *testing.T) {
	restoreDark(t)
	prev := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(prev) })

	now := time.Now()
	m := NewHomeModel()
	m.SetSize(100, 30)
	m.SetInstances([]*core.Instance{
		{ID: "a", Name: "26.2 Fabric", Version: "26.2", Loader: "fabric", LastPlayed: now.Add(-time.Hour)},
		{ID: "b", Name: "1.21.6 Fabric", Version: "1.21.6", Loader: "fabric", LastPlayed: now.Add(-2 * time.Hour)},
	}, "a")

	raw := strings.Split(m.View(), "\n")
	find := func(prefix string) string {
		for _, l := range raw {
			if strings.HasPrefix(strings.TrimRight(ansi.Strip(l), " "), prefix) {
				return l
			}
		}
		t.Fatalf("no line starting with %q in:\n%s", prefix, ansi.Strip(m.View()))
		return ""
	}
	find(SelectMark + " 26.2 Fabric")
	find("  26.2 · Fabric · 1h ago")
	find("  1.21.6 Fabric")
	find("  1.21.6 · Fabric · 2h ago")
	find("  + New instance")
}
