package ui

import (
	"errors"
	"fmt"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
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
	if m.loaderIndex != 1 {
		t.Fatalf("down from the last loader should stay put (no wrap), got index %d", m.loaderIndex)
	}
	m.moveLoaderSelection(-1)
	m.moveLoaderSelection(-1)
	if m.loaderIndex != 0 {
		t.Fatalf("up past the first loader should stay on Fabric, got index %d", m.loaderIndex)
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

func wizardWithVersions(t *testing.T) *WizardModel {
	t.Helper()
	restoreDark(t)
	now := time.Now()
	m := NewWizardModel(false)
	m.SetSize(100, 30)
	m.SetVersions([]core.Version{
		{ID: "1.21.4", Type: core.VersionTypeRelease, ReleaseTime: now},
		{ID: "25w14a", Type: core.VersionTypeSnapshot, ReleaseTime: now.Add(-time.Hour)},
		{ID: "1.21.3", Type: core.VersionTypeRelease, ReleaseTime: now.Add(-2 * time.Hour)},
	}, "1.21.4")
	return m
}

// q on the version list used to reach the bubbles list's quit binding and exit
// the whole app.
func TestWizard_QDoesNotQuitFromVersionList(t *testing.T) {
	m := wizardWithVersions(t)
	_, cmd := m.Update(keyMsg("q"))
	if cmd != nil {
		if _, quit := cmd().(tea.QuitMsg); quit {
			t.Fatal("q on the version step must not quit the app")
		}
	}
}

// Esc with a filter active clears the filter instead of leaving the wizard.
func TestWizard_EscClearsFilterFirst(t *testing.T) {
	m := wizardWithVersions(t)
	m.Update(keyMsg("/"))
	if m.versionList.FilterState() == list.Unfiltered {
		t.Fatal("setup: / should start filtering")
	}
	_, cmd := m.Update(keyMsg("esc"))
	if cmd != nil {
		if _, home := cmd().(NavigateToHome); home {
			t.Fatal("esc while filtering must not leave the wizard")
		}
	}
	if m.versionList.FilterState() != list.Unfiltered {
		t.Error("esc should clear the filter")
	}
}

// Toggling snapshots keeps the selected version and the "latest" mark.
func TestWizard_SnapshotToggleKeepsSelectionAndLatest(t *testing.T) {
	m := wizardWithVersions(t)
	m.versionList.Select(1) // 1.21.3
	m.Update(keyMsg(" "))
	if !m.showSnaps || len(m.versionList.Items()) != 3 {
		t.Fatalf("space should show snapshots (showSnaps=%v items=%d)", m.showSnaps, len(m.versionList.Items()))
	}
	if it, _ := m.versionList.SelectedItem().(versionItem); it.version.ID != "1.21.3" {
		t.Errorf("selection moved to %q, want 1.21.3", it.version.ID)
	}
	if it, _ := m.versionList.Items()[0].(versionItem); !it.latest {
		t.Error("the latest release lost its mark after the toggle")
	}
}

// Going back to the loader step and forward again keeps a name the user typed.
func TestWizard_BackAndForthKeepsTypedName(t *testing.T) {
	m := wizardWithVersions(t)
	m.Update(keyMsg("enter")) // version -> loader
	m.Update(keyMsg("enter")) // loader (Fabric) -> name
	m.nameInput.SetValue("My World")
	m.Update(keyMsg("esc"))   // back to loader
	m.Update(keyMsg("enter")) // forward again
	if got := m.nameInput.Value(); got != "My World" {
		t.Errorf("name = %q, want the typed %q", got, "My World")
	}
}

// Enter while typing a version filter applies it; it doesn't pick the top match
// and jump to the loader step.
func TestWizard_EnterAppliesFilterFirst(t *testing.T) {
	m := wizardWithVersions(t)
	m.Update(keyMsg("/"))
	m.Update(keyMsg("enter"))
	if m.step != StepSelectVersion {
		t.Fatalf("enter while filtering advanced to step %d", m.step)
	}
}

// Versions usually arrive after the wizard is sized. The version step must
// still fit the terminal (bubbles measured the page dots before it knew there
// was more than one page, and drew one row too many).
func TestWizard_FitsWhenVersionsArriveAfterSize(t *testing.T) {
	restoreDark(t)
	var vs []core.Version
	for i := range 40 {
		vs = append(vs, core.Version{ID: fmt.Sprintf("1.%d", i), Type: core.VersionTypeRelease, ReleaseTime: time.Now()})
	}
	for h := 16; h <= 30; h++ {
		m := NewWizardModel(false)
		m.SetSize(76, h)
		m.SetVersions(vs, "1.0")
		if got := lipgloss.Height(m.View()); got > h {
			t.Errorf("height %d: view is %d rows", h, got)
		}
	}
}
