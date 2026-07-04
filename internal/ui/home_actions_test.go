package ui

import (
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	tea "github.com/charmbracelet/bubbletea"
)

func homeWithInstances(t *testing.T) *HomeModel {
	t.Helper()
	m := NewHomeModel()
	m.SetInstances([]*core.Instance{
		{ID: "a", Name: "Survival World", Version: "1.21.4", Loader: "fabric", Path: t.TempDir()},
		{ID: "b", Name: "Creative Flats", Version: "1.21", Loader: "vanilla", Path: t.TempDir()},
	}, "a")
	m.SetSize(100, 30)
	return m
}

// `x` opens the actions popup; `e` from the popup emits NavigateToEditInstance;
// esc closes it.
func TestHomeActionsPopupOpenEditClose(t *testing.T) {
	m := homeWithInstances(t)

	if m.actionsPopup {
		t.Fatal("popup should start closed")
	}
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("x")})
	if !m.actionsPopup {
		t.Fatal("x should open the actions popup")
	}
	if m.actionsTarget == nil || m.actionsTarget.ID != "a" {
		t.Fatalf("popup target should be the selected instance, got %v", m.actionsTarget)
	}

	// `e` from the popup → NavigateToEditInstance, and closes the popup.
	_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("e")})
	if m.actionsPopup {
		t.Error("choosing an action should close the popup")
	}
	if cmd == nil {
		t.Fatal("expected a navigate command")
	}
	if _, ok := cmd().(NavigateToEditInstance); !ok {
		t.Fatalf("expected NavigateToEditInstance, got %T", cmd())
	}

	// Reopen and esc to close.
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("x")})
	if !m.actionsPopup {
		t.Fatal("x should reopen the popup")
	}
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if m.actionsPopup {
		t.Error("esc should close the popup")
	}
}

// `d` from the popup hands off to the existing delete-confirm overlay rather
// than duplicating delete logic.
func TestHomeActionsPopupDeleteEntersConfirm(t *testing.T) {
	m := homeWithInstances(t)
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("x")})
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("d")})

	if m.actionsPopup {
		t.Error("delete should close the actions popup")
	}
	if !m.confirmDelete {
		t.Fatal("delete should enter the confirm-delete overlay")
	}
	if m.deleteTarget == nil || m.deleteTarget.ID != "a" {
		t.Fatalf("confirm target should be the selected instance, got %v", m.deleteTarget)
	}
}

// Bare `e` on home edits the selected instance directly.
func TestHomeBareEditKey(t *testing.T) {
	m := homeWithInstances(t)
	_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("e")})
	if cmd == nil {
		t.Fatal("expected a navigate command from bare e")
	}
	if _, ok := cmd().(NavigateToEditInstance); !ok {
		t.Fatalf("expected NavigateToEditInstance, got %T", cmd())
	}
}

// Opening the actions popup is blocked while delete-confirm is active (mutual
// exclusion): keys go to the active overlay, not the home switch.
func TestHomeOverlayMutualExclusion(t *testing.T) {
	m := homeWithInstances(t)
	// Enter delete-confirm via the `d` key on home.
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("d")})
	if !m.confirmDelete {
		t.Fatal("d should arm delete-confirm")
	}
	// While confirm is up, `x` must not open the actions popup.
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("x")})
	if m.actionsPopup {
		t.Error("x must not open the actions popup while delete-confirm is active")
	}
}
