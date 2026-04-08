package ui

import (
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func TestValidateInstanceName(t *testing.T) {
	existing := map[string]struct{}{
		"vanilla": {},
	}

	cases := []struct {
		name    string
		wantErr bool
	}{
		{name: "My Instance", wantErr: false},
		{name: "vanilla", wantErr: true},
		{name: "Vanilla", wantErr: true},
		{name: ".", wantErr: true},
		{name: "..", wantErr: true},
		{name: "bad/char", wantErr: true},
		{name: "bad\\char", wantErr: true},
		{name: "bad:char", wantErr: true},
		{name: "bad*char", wantErr: true},
		{name: "trailingspace ", wantErr: true},
		{name: "trailingperiod.", wantErr: true},
		{name: "contains\tcontrol", wantErr: true},
	}

	for _, tc := range cases {
		err := validateInstanceName(tc.name, existing)
		if tc.wantErr && err == nil {
			t.Fatalf("expected error for %q, got nil", tc.name)
		}
		if !tc.wantErr && err != nil {
			t.Fatalf("unexpected error for %q: %v", tc.name, err)
		}
	}
}

func TestNameStep_spaceTogglesFabricStarterCheckbox(t *testing.T) {
	m := NewWizardModel(nil)
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
