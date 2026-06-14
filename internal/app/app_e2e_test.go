package app

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/ui"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/exp/teatest"
)

// waitDuration bounds every teatest.WaitFor assertion. Generous enough for CI,
// short enough that a genuinely stuck flow fails fast instead of hanging.
const waitDuration = 3 * time.Second

// newTestModel builds a fully wired *Model backed by a temp data dir, with no
// accounts (so the active-session check short-circuits to NotApplicable and never
// hits the network). The Modrinth client is the real one; none of the scenarios
// here touch it. The global theme is snapshotted and restored in cleanup, so
// theme-mutating tests must NOT run in parallel.
func newTestModel(t *testing.T) *Model {
	t.Helper()

	tmp := t.TempDir()
	cfg := &config.Config{
		DataDir:            tmp,
		InstancesDir:       filepath.Join(tmp, "instances"),
		AssetsDir:          filepath.Join(tmp, "assets"),
		LibrariesDir:       filepath.Join(tmp, "libraries"),
		JVMArgs:            config.DefaultJVMArgs(),
		Theme:              "dark",
		MSAClientID:        config.DefaultMSAClientID,
		LaunchLogVerbosity: "error",
	}
	if err := cfg.EnsureDirs(); err != nil {
		t.Fatalf("EnsureDirs: %v", err)
	}

	// New() applies the configured theme; replicate that for the injected path so
	// views render against a concrete palette. Restore the prior global theme.
	orig := ui.ActiveName()
	t.Cleanup(func() { ui.Apply(orig) })
	ui.Apply(cfg.Theme)

	instances := core.NewInstanceManager(cfg.DataDir)
	accounts := core.NewAccountManager(cfg.DataDir) // empty: GetActive() == nil

	return newWithDeps(
		cfg,
		instances,
		accounts,
		api.NewMojangClient(cfg.DataDir),
		api.NewModrinthClientWithBaseURL("http://127.0.0.1:0"),
	)
}

// keyRunes sends a letter / rune keypress (e.g. "n", "s", "y").
func keyRunes(s string) tea.KeyMsg {
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(s)}
}

// waitForOutput blocks until the program's rendered output contains sub.
func waitForOutput(t *testing.T, tm *teatest.TestModel, sub string) {
	t.Helper()
	teatest.WaitFor(t, tm.Output(), func(b []byte) bool {
		return bytes.Contains(b, []byte(sub))
	}, teatest.WithDuration(waitDuration))
}

// S1: boot renders Home; "n" enters the wizard; Esc returns Home; "q" quits.
func TestE2E_BootAndNavigateWizard(t *testing.T) {
	m := newTestModel(t)
	tm := teatest.NewTestModel(t, m, teatest.WithInitialTermSize(120, 40))

	// Boot: with no instances, Home shows the empty state.
	waitForOutput(t, tm, "No instances yet")

	// Enter the new-instance wizard.
	tm.Send(keyRunes("n"))
	waitForOutput(t, tm, "Select Minecraft Version")

	// Esc backs out to Home.
	tm.Send(tea.KeyMsg{Type: tea.KeyEsc})
	waitForOutput(t, tm, "No instances yet")

	// "q" on Home quits.
	tm.Send(keyRunes("q"))
	tm.WaitFinished(t, teatest.WithFinalTimeout(waitDuration))
}

// S2: drive the wizard end to end with injected versions (no Mojang call) and
// assert the new instance lands on Home and instance.json exists on disk.
func TestE2E_WizardCreatesInstance(t *testing.T) {
	m := newTestModel(t)
	instancesDir := m.cfg.InstancesDir
	tm := teatest.NewTestModel(t, m, teatest.WithInitialTermSize(120, 40))

	waitForOutput(t, tm, "No instances yet")

	// Enter the wizard.
	tm.Send(keyRunes("n"))
	waitForOutput(t, tm, "Select Minecraft Version")

	// Bypass Mojang by injecting the version-manifest message the wizard consumes.
	now := time.Now()
	tm.Send(ui.VersionsLoaded{
		Versions: []core.Version{
			{ID: "1.21.4", Type: core.VersionTypeRelease, ReleaseTime: now},
			{ID: "1.21.3", Type: core.VersionTypeRelease, ReleaseTime: now.Add(-time.Hour)},
		},
		Latest: "1.21.4",
	})
	// Version list now populated (latest is starred).
	waitForOutput(t, tm, "1.21.4")

	// Pick the highlighted version -> loader step.
	tm.Send(tea.KeyMsg{Type: tea.KeyEnter})
	waitForOutput(t, tm, "Mod loader")

	// Fabric is the first (highlighted) loader -> name step.
	tm.Send(tea.KeyMsg{Type: tea.KeyEnter})
	waitForOutput(t, tm, "Name your instance")

	// The name field is prefilled ("1.21.4 Fabric"); submit it.
	tm.Send(tea.KeyMsg{Type: tea.KeyEnter})

	// Back on Home, the new instance is listed by name.
	waitForOutput(t, tm, "1.21.4 Fabric")

	tm.Send(keyRunes("q"))
	tm.WaitFinished(t, teatest.WithFinalTimeout(waitDuration))

	// instance.json must exist under the temp instances dir.
	if !instanceJSONExists(t, instancesDir) {
		t.Fatalf("expected an instance.json under %s after creation", instancesDir)
	}
}

// S4: change + save the theme in Settings; assert it persists to config.json.
func TestE2E_SettingsThemePersists(t *testing.T) {
	m := newTestModel(t)
	configPath := filepath.Join(m.cfg.DataDir, "config.json")
	tm := teatest.NewTestModel(t, m, teatest.WithInitialTermSize(120, 40))

	waitForOutput(t, tm, "No instances yet")

	// Open Settings.
	tm.Send(keyRunes("s"))
	waitForOutput(t, tm, "Settings")

	// Focus order: JavaPath, JVMArgs, Snapshots, Theme, MSAClientID, Save.
	// Tab x3 -> Theme row.
	tm.Send(tea.KeyMsg{Type: tea.KeyTab})
	tm.Send(tea.KeyMsg{Type: tea.KeyTab})
	tm.Send(tea.KeyMsg{Type: tea.KeyTab})

	// Right cycles the theme (live preview). Seeded at "dark" (idx 1 of
	// auto,dark,light,...), so one Right -> "light".
	tm.Send(tea.KeyMsg{Type: tea.KeyRight})

	// Tab x2 -> Save, then Enter submits.
	tm.Send(tea.KeyMsg{Type: tea.KeyTab})
	tm.Send(tea.KeyMsg{Type: tea.KeyTab})
	tm.Send(tea.KeyMsg{Type: tea.KeyEnter})

	// Saving returns to Home. (The empty-state Home view doesn't render the
	// transient "Settings saved." banner, so assert the Home empty state instead;
	// the persisted theme below proves the save actually ran.)
	waitForOutput(t, tm, "No instances yet")

	tm.Send(keyRunes("q"))
	tm.WaitFinished(t, teatest.WithFinalTimeout(waitDuration))

	data, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatalf("read config.json: %v", err)
	}
	if !strings.Contains(string(data), `"theme": "light"`) {
		t.Fatalf("config.json did not persist the changed theme; got:\n%s", data)
	}
}

// S5: delete confirmation. Seed one instance on disk, then confirm + cancel.
func TestE2E_HomeDeleteConfirm(t *testing.T) {
	t.Run("confirm deletes", func(t *testing.T) {
		m := newTestModel(t)
		seedInstance(t, m.instances, "Seeded Pack")
		instDir := singleInstanceDir(t, m.cfg.InstancesDir)

		tm := teatest.NewTestModel(t, m, teatest.WithInitialTermSize(120, 40))
		waitForOutput(t, tm, "Seeded Pack")

		// Open the delete confirmation modal.
		tm.Send(keyRunes("d"))
		waitForOutput(t, tm, "Delete instance?")

		// Confirm with "y".
		tm.Send(keyRunes("y"))
		waitForOutput(t, tm, "No instances yet")

		tm.Send(keyRunes("q"))
		tm.WaitFinished(t, teatest.WithFinalTimeout(waitDuration))

		if _, err := os.Stat(instDir); !os.IsNotExist(err) {
			t.Fatalf("instance dir should be gone after confirm, stat err = %v", err)
		}
	})

	t.Run("esc cancels", func(t *testing.T) {
		m := newTestModel(t)
		seedInstance(t, m.instances, "Keeper Pack")
		instDir := singleInstanceDir(t, m.cfg.InstancesDir)

		tm := teatest.NewTestModel(t, m, teatest.WithInitialTermSize(120, 40))
		waitForOutput(t, tm, "Keeper Pack")

		tm.Send(keyRunes("d"))
		waitForOutput(t, tm, "Delete instance?")

		// Esc cancels; the instance remains listed.
		tm.Send(tea.KeyMsg{Type: tea.KeyEsc})
		waitForOutput(t, tm, "Keeper Pack")

		tm.Send(keyRunes("q"))
		tm.WaitFinished(t, teatest.WithFinalTimeout(waitDuration))

		if _, err := os.Stat(instDir); err != nil {
			t.Fatalf("instance dir should still exist after cancel: %v", err)
		}
	})
}

// --- helpers ---

// seedInstance creates an instance on disk via the manager before the model boots.
func seedInstance(t *testing.T, im *core.InstanceManager, name string) {
	t.Helper()
	if err := im.Create(&core.Instance{
		Name:    name,
		Version: "1.21.4",
		Loader:  "vanilla",
	}); err != nil {
		t.Fatalf("seed instance %q: %v", name, err)
	}
}

// singleInstanceDir returns the path of the sole instance directory under
// instancesDir, failing if there isn't exactly one.
func singleInstanceDir(t *testing.T, instancesDir string) string {
	t.Helper()
	entries, err := os.ReadDir(instancesDir)
	if err != nil {
		t.Fatalf("read instances dir: %v", err)
	}
	var dirs []string
	for _, e := range entries {
		if e.IsDir() {
			dirs = append(dirs, filepath.Join(instancesDir, e.Name()))
		}
	}
	if len(dirs) != 1 {
		t.Fatalf("expected exactly one instance dir, found %d in %s", len(dirs), instancesDir)
	}
	return dirs[0]
}

// instanceJSONExists reports whether any instance subdir contains an instance.json.
func instanceJSONExists(t *testing.T, instancesDir string) bool {
	t.Helper()
	entries, err := os.ReadDir(instancesDir)
	if err != nil {
		return false
	}
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		if _, err := os.Stat(filepath.Join(instancesDir, e.Name(), "instance.json")); err == nil {
			return true
		}
	}
	return false
}
