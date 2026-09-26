package ui

import (
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/x/ansi"
)

// Startup recovery / crash notices must be visible on both the empty state and
// the list, stay until a key press, and not swallow that key.
func TestHome_NoticeStaysUntilAKeyAndShowsWhenEmpty(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(80, 24)
	m.SetInstances(nil, "")
	m.SetNotice("config.json was damaged — started with default settings.")
	if !strings.Contains(ansi.Strip(m.View()), "config.json was damaged") {
		t.Fatal("the notice should render on the empty-state screen")
	}

	m.SetInstances([]*core.Instance{{ID: "a", Name: "Alpha", Version: "1.21.4"}}, "")
	view := ansi.Strip(m.View())
	if !strings.Contains(view, "config.json was damaged") {
		t.Fatal("the notice should render above the footer")
	}
	if h := strings.Count(view, "\n") + 1; h > 24 {
		t.Errorf("view is %d rows with the notice, want ≤ 24", h)
	}

	_, cmd := m.Update(keyMsg("n"))
	if strings.Contains(ansi.Strip(m.View()), "config.json was damaged") {
		t.Error("a key press should dismiss the notice")
	}
	if cmd == nil {
		t.Fatal("the dismissing key should still act (n = new instance)")
	}
	if _, ok := cmd().(NavigateToNewInstance); !ok {
		t.Errorf("n should still open the wizard, got %T", cmd())
	}
}
