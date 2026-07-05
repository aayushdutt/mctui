package ui

import (
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	tea "github.com/charmbracelet/bubbletea"
)

// The key grammar (spec M1.1): Enter = affirmative only, Esc = one step back,
// Tab/Shift+Tab = cycle panes/fields, and h/j/k/l mirror the arrows. These tests
// pin the behavior changes so the grammar can't silently regress.

// CanBrowseMods is the single mods gate (spec M4.3): Fabric passes, everything
// else is blocked with a banner reason.
func TestCanBrowseMods(t *testing.T) {
	if ok, _ := CanBrowseMods(&core.Instance{Loader: "fabric"}); !ok {
		t.Fatal("fabric should clear the gate")
	}
	if ok, reason := CanBrowseMods(&core.Instance{Loader: "vanilla"}); ok || reason == "" {
		t.Fatalf("vanilla should be gated with a reason, got ok=%v reason=%q", ok, reason)
	}
	if ok, reason := CanBrowseMods(nil); ok || reason == "" {
		t.Fatalf("nil instance should be gated with a reason, got ok=%v reason=%q", ok, reason)
	}
}

func newModsWith(t *testing.T) *ModsModel {
	t.Helper()
	restoreDark(t)
	inst := fabricInstance(t)[0]
	if ok, _ := CanBrowseMods(inst); !ok {
		t.Fatal("a fabric instance should clear the mods gate")
	}
	m := NewModsModel(inst, api.NewModrinthClient())
	if len(m.installed.Items()) == 0 {
		t.Fatal("expected the fabric instance's installed jar to load")
	}
	return m
}

func sendMods(t *testing.T, m *ModsModel, s string) (*ModsModel, tea.Cmd) {
	t.Helper()
	nm, cmd := m.Update(keyMsg(s))
	return nm.(*ModsModel), cmd
}

// Enter on the installed pane must NOT open the remove dialog (Enter is
// affirmative-only; d / backspace remove).
func TestMods_EnterOnInstalledIsNoop(t *testing.T) {
	m := newModsWith(t)
	m.modsFocus = panelInstalled
	m, _ = sendMods(t, m, "enter")
	if m.modsDialog != modsDialogNone {
		t.Fatalf("Enter on the installed pane must be a no-op, got dialog %d", m.modsDialog)
	}
}

// d (and backspace) open the remove-confirm dialog on the installed pane.
func TestMods_DAndBackspaceRemoveFromInstalled(t *testing.T) {
	for _, key := range []string{"d", "backspace"} {
		m := newModsWith(t)
		m.modsFocus = panelInstalled
		if key == "backspace" {
			m, _ = func() (*ModsModel, tea.Cmd) {
				nm, cmd := m.Update(tea.KeyMsg{Type: tea.KeyBackspace})
				return nm.(*ModsModel), cmd
			}()
		} else {
			m, _ = sendMods(t, m, key)
		}
		if m.modsDialog != modsDialogConfirmRemoveJar {
			t.Fatalf("%q on the installed pane should open the remove dialog, got %d", key, m.modsDialog)
		}
	}
}

// h/l switch panes in the mods browser (vim parity with ←/→).
func TestMods_VimHLSwitchesPanes(t *testing.T) {
	cases := []struct {
		from modsPanel
		key  string
		want modsPanel
	}{
		{panelInstalled, "l", panelBrowse},
		{panelBrowse, "h", panelInstalled},
	}
	for _, tc := range cases {
		m := newModsWith(t)
		m.modsFocus = tc.from
		m, _ = sendMods(t, m, tc.key)
		if m.modsFocus != tc.want {
			t.Fatalf("%q from pane %d: got %d, want %d", tc.key, tc.from, m.modsFocus, tc.want)
		}
	}
}

// The shared command palette (spec M1.2) opens on non-home screens too, but ":"
// must type a literal colon while the search field is focused rather than open it.
func TestMods_PaletteOpensButNotWhileTyping(t *testing.T) {
	m := newModsWith(t)
	m.modsFocus = panelBrowse
	m, _ = sendMods(t, m, ":")
	if !m.palette.Active {
		t.Fatal(": should open the palette on the mods screen")
	}

	m2 := newModsWith(t)
	m2.modsFocus = panelQuery
	m2, _ = sendMods(t, m2, ":")
	if m2.palette.Active {
		t.Fatal(": must not open the palette while the search field is focused")
	}
}

// Esc on the home list (unfiltered) is one step back — since the list is already
// the root, it surfaces a quit hint rather than dying silently.
func TestHome_EscInListFocusHints(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	if m.paneFocus != focusList {
		t.Fatalf("expected focusList by default, got %d", m.paneFocus)
	}
	sendKey(t, m, "esc")
	if m.transientBanner == "" {
		t.Fatal("Esc on the unfiltered home list should show a quit hint banner")
	}
}

// ? opens the read-only help overlay; ? or esc dismisses it (spec M1.3).
func TestHome_HelpOverlayToggles(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	if m.help {
		t.Fatal("help overlay should start closed")
	}
	sendKey(t, m, "?")
	if !m.help {
		t.Fatal("? should open the help overlay")
	}
	// A stray key does not dismiss the read-only overlay.
	sendKey(t, m, "j")
	if !m.help {
		t.Fatal("help overlay should stay open on unrelated keys")
	}
	sendKey(t, m, "esc")
	if m.help {
		t.Fatal("esc should close the help overlay")
	}
	// Home help groups are built from the keymap and must be non-empty.
	if got := m.helpGroups(); len(got) == 0 || len(got[0].Entries) == 0 {
		t.Fatal("home help groups should be populated from the keymap")
	}
}

// Space (not Tab) toggles the snapshot filter on the wizard version step, and it
// still persists the preference. Tab is reserved for field/pane cycling.
func TestWizard_SpaceTogglesAndPersistsSnapshots(t *testing.T) {
	m := NewWizardModel(false)
	m.step = StepSelectVersion

	// Tab must NOT toggle snapshots anymore.
	next, _ := m.Update(tea.KeyMsg(tea.Key{Type: tea.KeyTab}))
	if next.(*WizardModel).showSnaps {
		t.Fatal("Tab on the version step must no longer toggle snapshots")
	}

	// Space toggles and persists.
	next, cmd := m.Update(tea.KeyMsg(tea.Key{Type: tea.KeySpace}))
	w := next.(*WizardModel)
	if !w.showSnaps {
		t.Fatal("Space on the version step should toggle showSnaps on")
	}
	if cmd == nil {
		t.Fatal("expected a PersistShowSnapshots command")
	}
	p, ok := cmd().(PersistShowSnapshots)
	if !ok {
		t.Fatalf("expected PersistShowSnapshots, got %T", cmd())
	}
	if !p.Value {
		t.Fatalf("PersistShowSnapshots.Value = %v, want true", p.Value)
	}
}
