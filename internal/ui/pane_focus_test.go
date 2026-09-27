package ui

import (
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
)

// Only the focused pane shows a selected row: the › marker follows keyboard
// focus, so an unfocused pane never looks like it's the one the arrows drive.

func TestMods_OnlyFocusedPaneShowsSelection(t *testing.T) {
	m := newModsWith(t)
	m.SetSize(120, 40)
	m.results.SetItems(m.hitsToItems([]api.SearchHit{{ProjectID: "p1", Slug: "lithium", Title: "Lithium"}}))

	m.modsFocus = panelBrowse
	m.View()
	if strings.Contains(m.installed.View(), SelectMark) {
		t.Error("installed pane shows a selection while results are focused")
	}
	if !strings.Contains(m.results.View(), SelectMark) {
		t.Error("focused results pane lost its selection")
	}

	m.modsFocus = panelInstalled
	m.View()
	if !strings.Contains(m.installed.View(), SelectMark) {
		t.Error("focused installed pane lost its selection")
	}
	if strings.Contains(m.results.View(), SelectMark) {
		t.Error("results pane shows a selection while installed is focused")
	}

	// The search field sits in the Discover pane, but it isn't the list: no
	// selected result while typing.
	m.modsFocus = panelQuery
	m.View()
	if strings.Contains(m.results.View(), SelectMark) {
		t.Error("results pane shows a selection while the search field is focused")
	}
}

func TestResourcePacks_OnlyFocusedPaneShowsSelection(t *testing.T) {
	restoreDark(t)
	m := rpRenderModel(t, 120, 40, map[string][]string{"Aesthetic": {"ClearGlass"}})
	lists := map[rpPanel]func() string{
		rpPanelCategories: func() string { return m.categories.View() },
		rpPanelPacks:      func() string { return m.packs.View() },
		rpPanelCart:       func() string { return m.cart.View() },
	}
	for focus := range lists {
		m.rpFocus = focus
		m.View()
		for pane, view := range lists {
			if got, want := strings.Contains(view(), SelectMark), pane == focus; got != want {
				t.Errorf("focus %d: pane %d shows selection = %v, want %v", focus, pane, got, want)
			}
		}
	}
}

func TestHome_ListHidesSelectionWhileDetailFocused(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	if !strings.Contains(m.list.View(), SelectMark) {
		t.Fatal("setup: the focused list should show its selection")
	}
	sendKey(t, m, "tab")
	if strings.Contains(m.list.View(), SelectMark) {
		t.Error("instance list shows a selection while the detail pane is focused")
	}
	sendKey(t, m, "esc")
	if !strings.Contains(m.list.View(), SelectMark) {
		t.Error("returning to the list should show its selection again")
	}
}
