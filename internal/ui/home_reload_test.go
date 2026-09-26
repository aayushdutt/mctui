package ui

import (
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
)

func reloadInstances(now time.Time) []*core.Instance {
	return []*core.Instance{
		{ID: "a", Name: "Alpha", LastPlayed: now.Add(-1 * time.Hour)},
		{ID: "b", Name: "Bravo", LastPlayed: now.Add(-2 * time.Hour)},
		{ID: "c", Name: "Charlie", LastPlayed: now.Add(-3 * time.Hour)},
	}
}

// runCmd feeds a command's message (and any batch) back into the model, the way
// the tea runtime would, so async list work (re-filtering) completes. Commands
// that don't return promptly (cursor-blink / banner ticks) are skipped.
func runCmd(m *HomeModel, cmd tea.Cmd) {
	if cmd == nil {
		return
	}
	done := make(chan tea.Msg, 1)
	go func() { done <- cmd() }()
	var msg tea.Msg
	select {
	case msg = <-done:
	case <-time.After(50 * time.Millisecond):
		return
	}
	switch msg := msg.(type) {
	case tea.BatchMsg:
		for _, c := range msg {
			runCmd(m, c)
		}
	case nil:
	default:
		m.Update(msg) // don't chase follow-ups (blinks/ticks)
	}
}

// The list re-sorts by recency on every reload; the cursor must follow the
// selected instance, not stay on its old row (Enter would launch another one).
func TestHome_ReloadKeepsSelectedInstance(t *testing.T) {
	restoreDark(t)
	now := time.Now()
	m := NewHomeModel()
	m.SetSize(120, 40)
	m.SetInstances(reloadInstances(now), "c")
	if got := m.SelectedInstance(); got == nil || got.ID != "c" {
		t.Fatalf("setup: selected %v, want c", got)
	}

	// c was just played, so it sorts to the top on reload.
	reloaded := reloadInstances(now)
	reloaded[2].LastPlayed = now
	m.SetInstances(reloaded, "")
	if got := m.SelectedInstance(); got == nil || got.ID != "c" {
		t.Errorf("after reload selected %v, want c", got)
	}
}

// A palette jump moves the cursor without going through the list; a reload must
// still keep that instance (not snap back to the one selected before the jump).
func TestHome_ReloadAfterPaletteJump(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 40)
	m.SetInstances(reloadInstances(time.Now()), "a")
	m.jumpToInstance(2) // Charlie
	m.SetInstances(reloadInstances(time.Now()), "")
	if got := m.SelectedInstance(); got == nil || got.ID != "c" {
		t.Errorf("after reload selected %v, want c (the jump target)", got)
	}
}

// On the "+ New instance" row, a reload (e.g. returning from the wizard) keeps
// it selected, so Enter reopens the wizard instead of launching an instance.
func TestHome_ReloadKeepsAddRow(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 40)
	m.SetInstances(reloadInstances(time.Now()), "a")
	m.list.Select(len(m.list.Items()) - 1)
	if !m.addRowSelected() {
		t.Fatal("setup: expected the + New row selected")
	}
	m.SetInstances(reloadInstances(time.Now()), "")
	if !m.addRowSelected() {
		t.Errorf("after reload selected %v, want the + New row", m.SelectedInstance())
	}
}

// With a filter applied, a reload (or the playing marker changing) used to leave
// the list showing "No items": SetItems clears the filtered view and returns the
// command that rebuilds it, which was dropped.
func TestHome_ReloadKeepsFilteredView(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 40)
	m.SetInstances(reloadInstances(time.Now()), "a")
	for _, k := range []tea.KeyMsg{keyMsg("/"), keyMsg("c"), keyMsg("h"), keyMsg("a"), keyMsg("enter")} {
		_, cmd := m.Update(k) // enter applies the filter
		runCmd(m, cmd)
	}
	if m.list.FilterState() != list.FilterApplied || len(m.list.VisibleItems()) != 1 {
		t.Fatalf("setup: want 1 visible item under an applied filter, got %d (%v)", len(m.list.VisibleItems()), m.list.FilterState())
	}

	runCmd(m, m.SetInstances(reloadInstances(time.Now()), ""))
	if n := len(m.list.VisibleItems()); n != 1 {
		t.Errorf("after reload %d visible items, want 1", n)
	}
	runCmd(m, m.SetPlaying("c"))
	if n := len(m.list.VisibleItems()); n != 1 {
		t.Errorf("after SetPlaying %d visible items, want 1", n)
	}
}
