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
	if !m.palette {
		t.Fatal(": should open the palette")
	}

	// Type "creat" → fuzzy-match "Creative Flats" to the top.
	typeRunes(m, "creat")
	if len(m.paletteEntries) == 0 {
		t.Fatal("expected fuzzy matches for 'creat'")
	}
	top := m.paletteEntries[0]
	if top.kind != paletteInstance || top.label != "Creative Flats" {
		t.Fatalf("expected 'Creative Flats' ranked first, got %+v", top)
	}

	// Enter activates the highlighted entry → moves the list cursor and closes.
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if m.palette {
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
	if len(m.paletteEntries) == 0 || m.paletteEntries[0].action != "settings" {
		t.Fatalf("expected 'settings' ranked first, got %+v", m.paletteEntries)
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
	if len(m.paletteEntries) == 0 || m.paletteEntries[0].action != "mods" {
		t.Fatalf("expected 'mods' ranked first, got %+v", m.paletteEntries)
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

// Esc closes the palette.
func TestHomePaletteEscCloses(t *testing.T) {
	m := homeWithInstances(t)
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(":")})
	if !m.palette {
		t.Fatal(": should open the palette")
	}
	_, _ = m.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if m.palette {
		t.Error("esc should close the palette")
	}
}
