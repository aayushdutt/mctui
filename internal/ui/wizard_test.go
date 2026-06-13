package ui

import (
	"errors"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func TestValidateInstanceName(t *testing.T) {
	// Display names are freeform now: path characters, duplicates, and trailing
	// spaces/periods are allowed (folder safety/uniqueness is handled at Create).
	// Only empty/whitespace-only and control characters are rejected.
	cases := []struct {
		name    string
		wantErr bool
	}{
		{name: "My Instance", wantErr: false},
		{name: "bad/char", wantErr: false},
		{name: "bad\\char", wantErr: false},
		{name: "bad:char", wantErr: false},
		{name: "bad*char", wantErr: false},
		{name: "trailingspace ", wantErr: false},
		{name: "trailingperiod.", wantErr: false},
		{name: ".", wantErr: false},
		{name: "café 🎮", wantErr: false},
		{name: "", wantErr: true},
		{name: "   ", wantErr: true},
		{name: "contains\tcontrol", wantErr: true},
	}

	for _, tc := range cases {
		err := validateInstanceName(tc.name)
		if tc.wantErr && err == nil {
			t.Fatalf("expected error for %q, got nil", tc.name)
		}
		if !tc.wantErr && err != nil {
			t.Fatalf("unexpected error for %q: %v", tc.name, err)
		}
	}
}

func TestNameStep_spaceTogglesFabricStarterCheckbox(t *testing.T) {
	m := NewWizardModel(false)
	m.step = StepEnterName
	m.selectedLoader = "fabric"
	m.nameStepSetFocus(focusWizardStarterCheckbox)
	m.installStarterMods = false

	keyTypes := []tea.Key{
		{Type: tea.KeySpace},
		{Type: tea.KeyRunes, Runes: []rune{' '}},
	}
	for _, k := range keyTypes {
		m.installStarterMods = false
		next, _ := m.Update(tea.KeyMsg(k))
		w := next.(*WizardModel)
		if !w.installStarterMods {
			t.Fatalf("key %+v should toggle installStarterMods on", k)
		}
	}
}

func TestNewWizardModel_SeedsShowSnapshots(t *testing.T) {
	if !NewWizardModel(true).showSnaps {
		t.Fatal("showSnaps should be seeded true")
	}
	if NewWizardModel(false).showSnaps {
		t.Fatal("showSnaps should be seeded false")
	}
}

func TestWizard_TabTogglesAndPersistsSnapshots(t *testing.T) {
	m := NewWizardModel(false)
	m.step = StepSelectVersion

	next, cmd := m.Update(tea.KeyMsg(tea.Key{Type: tea.KeyTab}))
	w := next.(*WizardModel)
	if !w.showSnaps {
		t.Fatal("Tab on the version step should toggle showSnaps on")
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

func TestWizard_RetryKeyResetsErrorAndEmitsRetry(t *testing.T) {
	m := NewWizardModel(false)
	m.err = errors.New("boom")
	m.loading = false

	next, cmd := m.Update(tea.KeyMsg(tea.Key{Type: tea.KeyRunes, Runes: []rune{'r'}}))
	w := next.(*WizardModel)
	if w.err != nil {
		t.Fatalf("retry should clear err, got %v", w.err)
	}
	if !w.loading {
		t.Fatal("retry should set loading true")
	}
	if cmd == nil {
		t.Fatal("expected a RetryLoadVersions command")
	}
	if _, ok := cmd().(RetryLoadVersions); !ok {
		t.Fatalf("expected RetryLoadVersions, got %T", cmd())
	}
}

func TestWizard_RetryKeyNoopWithoutError(t *testing.T) {
	m := NewWizardModel(false)
	m.err = nil

	// 'r' with no error must not emit RetryLoadVersions, so it stays usable as a filter char.
	_, cmd := m.Update(tea.KeyMsg(tea.Key{Type: tea.KeyRunes, Runes: []rune{'r'}}))
	if cmd != nil {
		if _, ok := cmd().(RetryLoadVersions); ok {
			t.Fatal("'r' without an error must not emit RetryLoadVersions")
		}
	}
}

func TestMoveLoaderSelection_skipsComingSoon(t *testing.T) {
	m := &WizardModel{
		loaderChoices: []loaderChoice{
			{Label: "Fabric", ID: "fabric"},
			{Label: "Vanilla", ID: "vanilla"},
			{Label: "Forge (coming soon)", ID: "", ComingSoon: true},
			{Label: "Quilt (coming soon)", ID: "", ComingSoon: true},
		},
		loaderIndex: 0,
	}
	m.moveLoaderSelection(1)
	if m.loaderIndex != 1 {
		t.Fatalf("down from Fabric: got index %d want 1 (Vanilla)", m.loaderIndex)
	}
	m.moveLoaderSelection(1)
	if m.loaderIndex != 0 {
		t.Fatalf("down from Vanilla should wrap to Fabric, got index %d", m.loaderIndex)
	}
	m.moveLoaderSelection(-1)
	if m.loaderIndex != 1 {
		t.Fatalf("up from Fabric should wrap to Vanilla, got index %d", m.loaderIndex)
	}
	m.loaderIndex = 3 // stale: on a coming-soon row
	m.moveLoaderSelection(1)
	if m.loaderIndex != 0 {
		t.Fatalf("snap from coming-soon: got %d want Fabric (0)", m.loaderIndex)
	}
	m.moveLoaderSelection(1)
	if m.loaderIndex != 1 {
		t.Fatalf("after snap, down → Vanilla: got %d", m.loaderIndex)
	}
}
