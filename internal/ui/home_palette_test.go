package ui

import (
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func typeRunes(m *HomeModel, s string) {
	for _, r := range s {
		_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}})
	}
}

// `:` opens the palette; typing filters via fuzzy; enter on an instance moves
// the list selection and closes; esc closes.
func TestHomePaletteJumpToInstance(t *testing.T) {
	m := homeWithInstances(t)
	// Selection starts on "Survival World" (id a).
	if m.SelectedInstance() == nil || m.SelectedInstance().ID != "a" {
		t.Fatalf("precondition: selection should be a, got %v", m.SelectedInstance())
	}

	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(":")})
	if !m.palette.Active {
		t.Fatal(": should open the palette")
	}

	// Type "creat" → fuzzy-match "Creative Flats" to the top.
	typeRunes(m, "creat")
	if len(m.palette.items) == 0 {
		t.Fatal("expected fuzzy matches for 'creat'")
	}
	top := m.palette.items[0]
	if top.Kind != PaletteJump || top.Label != "Creative Flats" {
		t.Fatalf("expected 'Creative Flats' ranked first, got %+v", top)
	}

	// Enter activates the highlighted entry → moves the list cursor and closes.
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if m.palette.Active {
		t.Error("enter should close the palette")
	}
	if m.SelectedInstance() == nil || m.SelectedInstance().ID != "b" {
		t.Fatalf("enter should jump selection to Creative Flats (b), got %v", m.SelectedInstance())
	}
}

// Typing a verb and pressing enter emits the matching Navigate message.
func TestHomePaletteVerbNavigate(t *testing.T) {
	m := homeWithInstances(t)
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(":")})
	typeRunes(m, "settings")
	if len(m.palette.items) == 0 || m.palette.items[0].Label != "Settings" {
		t.Fatalf("expected 'Settings' ranked first, got %+v", m.palette.items)
	}
	_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd == nil {
		t.Fatal("expected a navigate command")
	}
	if _, ok := cmd().(NavigateToSettings); !ok {
		t.Fatalf("expected NavigateToSettings, got %T", cmd())
	}
}

// The mods verb respects the browser gate: a vanilla selection shows the
// transient banner instead of navigating.
func TestHomePaletteModsGate(t *testing.T) {
	m := homeWithInstances(t)
	m.list.Select(1) // Creative Flats (vanilla)
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(":")})
	typeRunes(m, "mods")
	if len(m.palette.items) == 0 || m.palette.items[0].Label != "Mods" {
		t.Fatalf("expected 'Mods' ranked first, got %+v", m.palette.items)
	}
	_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd != nil {
		if _, ok := cmd().(NavigateToMods); ok {
			t.Fatal("mods should be gated for a vanilla instance")
		}
	}
	if m.transientBanner == "" {
		t.Error("gated mods should set the transient banner")
	}
}

// The palette gains launch verbs (spec M1.2): Play emits a launch command.
func TestHomePalettePlayVerb(t *testing.T) {
	m := homeWithInstances(t)
	// Offline account so Play routes straight to launch (no auth detour).
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(":")})
	typeRunes(m, "play offline")
	if len(m.palette.items) == 0 || m.palette.items[0].Label != "Play offline" {
		t.Fatalf("expected 'Play offline' ranked first, got %+v", m.palette.items)
	}
	_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd == nil {
		t.Fatal("Play offline should emit a launch command")
	}
	if nav, ok := cmd().(NavigateToLaunch); !ok || !nav.Offline {
		t.Fatalf("expected an offline NavigateToLaunch, got %T", cmd())
	}
}

// The Delete verb arms the delete-confirm overlay for the current selection.
func TestHomePaletteDeleteVerb(t *testing.T) {
	m := homeWithInstances(t)
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(":")})
	typeRunes(m, "delete")
	if len(m.palette.items) == 0 || m.palette.items[0].Label != "Delete instance" {
		t.Fatalf("expected 'Delete instance' ranked first, got %+v", m.palette.items)
	}
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if !m.confirmDelete || m.deleteTarget == nil {
		t.Fatal("Delete verb should arm the delete-confirm overlay")
	}
}

// Esc closes the palette.
func TestHomePaletteEscCloses(t *testing.T) {
	m := homeWithInstances(t)
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(":")})
	if !m.palette.Active {
		t.Fatal(": should open the palette")
	}
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if m.palette.Active {
		t.Error("esc should close the palette")
	}
}
