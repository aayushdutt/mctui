package ui

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/aayushdutt/mctui/internal/config"
	tea "github.com/charmbracelet/bubbletea"
)

func keyEnter() tea.Msg { return tea.KeyMsg(tea.Key{Type: tea.KeyEnter}) }

func TestNewSettingsModel_SeedsFromConfig(t *testing.T) {
	cfg := &config.Config{
		JavaPath:      "/opt/java",
		JVMArgs:       []string{"-Xmx8G", "-Xms2G"},
		ShowSnapshots: true,
		MSAClientID:   "abc",
	}
	m := NewSettingsModel(cfg)

	if got := m.javaPath.Value(); got != "/opt/java" {
		t.Fatalf("javaPath seed = %q, want /opt/java", got)
	}
	if got := m.jvmArgs.Value(); got != "-Xmx8G -Xms2G" {
		t.Fatalf("jvmArgs seed = %q, want %q", got, "-Xmx8G -Xms2G")
	}
	if !m.snapshots {
		t.Fatal("snapshots seed should be true")
	}
	if got := m.msaClientID.Value(); got != "abc" {
		t.Fatalf("msaClientID seed = %q, want abc", got)
	}
}

func TestSettings_SubmitEmitsSettingsSaved(t *testing.T) {
	m := NewSettingsModel(&config.Config{})
	m.javaPath.SetValue("")
	m.jvmArgs.SetValue("-Xmx4G -Xms1G")
	m.msaClientID.SetValue("  my-client-id  ")
	m.snapshots = true
	m.applyFocus(focusSettingsSave)

	_, cmd := m.Update(keyEnter())
	if cmd == nil {
		t.Fatal("expected a SettingsSaved command")
	}
	saved, ok := cmd().(SettingsSaved)
	if !ok {
		t.Fatalf("expected SettingsSaved, got %T", cmd())
	}
	if saved.JavaPath != "" {
		t.Fatalf("JavaPath = %q, want empty", saved.JavaPath)
	}
	if !saved.ShowSnapshots {
		t.Fatal("ShowSnapshots should be true")
	}
	if saved.MSAClientID != "my-client-id" {
		t.Fatalf("MSAClientID = %q, want trimmed my-client-id", saved.MSAClientID)
	}
	if want := []string{"-Xmx4G", "-Xms1G"}; !reflect.DeepEqual(saved.JVMArgs, want) {
		t.Fatalf("JVMArgs = %v, want %v", saved.JVMArgs, want)
	}
}

func TestSettings_JVMArgsSplitHandlesExtraSpaces(t *testing.T) {
	m := NewSettingsModel(&config.Config{})
	m.jvmArgs.SetValue("   -Xmx4G    -Xms1G   ")
	m.applyFocus(focusSettingsSave)

	_, cmd := m.Update(keyEnter())
	saved, ok := cmd().(SettingsSaved)
	if !ok {
		t.Fatalf("expected SettingsSaved, got %T", cmd())
	}
	if want := []string{"-Xmx4G", "-Xms1G"}; !reflect.DeepEqual(saved.JVMArgs, want) {
		t.Fatalf("JVMArgs = %v, want %v", saved.JVMArgs, want)
	}
}

func TestSettings_EmptyJavaPathSubmits(t *testing.T) {
	m := NewSettingsModel(&config.Config{})
	m.javaPath.SetValue("")
	m.applyFocus(focusSettingsSave)

	_, cmd := m.Update(keyEnter())
	if cmd == nil {
		t.Fatal("expected SettingsSaved")
	}
	if _, ok := cmd().(SettingsSaved); !ok {
		t.Fatalf("expected SettingsSaved, got %T", cmd())
	}
	if m.saveErr != "" {
		t.Fatalf("unexpected saveErr %q for empty java path", m.saveErr)
	}
}

func TestSettings_ValidJavaPathSubmits(t *testing.T) {
	p := filepath.Join(t.TempDir(), "java")
	if err := os.WriteFile(p, []byte("x"), 0o755); err != nil {
		t.Fatal(err)
	}
	m := NewSettingsModel(&config.Config{})
	m.javaPath.SetValue(p)
	m.applyFocus(focusSettingsSave)

	_, cmd := m.Update(keyEnter())
	saved, ok := cmd().(SettingsSaved)
	if !ok {
		t.Fatalf("expected SettingsSaved, got %T", cmd())
	}
	if saved.JavaPath != p {
		t.Fatalf("JavaPath = %q, want %q", saved.JavaPath, p)
	}
}

func TestSettings_InvalidJavaPathBlocksSubmit(t *testing.T) {
	missing := filepath.Join(t.TempDir(), "does-not-exist")
	m := NewSettingsModel(&config.Config{})
	m.javaPath.SetValue(missing)
	m.applyFocus(focusSettingsSave)

	_, cmd := m.Update(keyEnter())
	if m.saveErr == "" {
		t.Fatal("expected saveErr for a missing java path")
	}
	if m.focus != focusSettingsJavaPath {
		t.Fatalf("focus should return to java path on error, got %v", m.focus)
	}
	if cmd != nil {
		if _, ok := cmd().(SettingsSaved); ok {
			t.Fatal("a missing java path must not emit SettingsSaved")
		}
	}
}

func TestSettings_SpaceTogglesSnapshotsWhenFocused(t *testing.T) {
	m := NewSettingsModel(&config.Config{})
	m.snapshots = false
	m.applyFocus(focusSettingsSnapshots)

	next, _ := m.Update(tea.KeyMsg(tea.Key{Type: tea.KeySpace}))
	if !next.(*SettingsModel).snapshots {
		t.Fatal("space on the checkbox should toggle snapshots on")
	}
}

func TestSettings_SpaceDoesNotToggleWhenInputFocused(t *testing.T) {
	m := NewSettingsModel(&config.Config{})
	m.snapshots = false
	m.applyFocus(focusSettingsJavaPath)

	next, _ := m.Update(tea.KeyMsg(tea.Key{Type: tea.KeySpace}))
	if next.(*SettingsModel).snapshots {
		t.Fatal("space in a text field must not toggle snapshots")
	}
}

func TestSettings_EscNavigatesHome(t *testing.T) {
	m := NewSettingsModel(&config.Config{})
	_, cmd := m.Update(tea.KeyMsg(tea.Key{Type: tea.KeyEsc}))
	if cmd == nil {
		t.Fatal("expected a NavigateToHome command")
	}
	if _, ok := cmd().(NavigateToHome); !ok {
		t.Fatalf("expected NavigateToHome, got %T", cmd())
	}
}

func TestSettings_FocusCycleWraps(t *testing.T) {
	m := NewSettingsModel(&config.Config{})
	m.applyFocus(focusSettingsJavaPath)

	m.cycleFocus(-1)
	if m.focus != focusSettingsSave {
		t.Fatalf("shift back from first field should wrap to Save, got %v", m.focus)
	}
	m.cycleFocus(1)
	if m.focus != focusSettingsJavaPath {
		t.Fatalf("forward from Save should wrap to the first field, got %v", m.focus)
	}
}
