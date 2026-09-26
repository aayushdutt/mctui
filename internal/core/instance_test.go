package core

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func TestRecencyForSort(t *testing.T) {
	t1 := time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC)
	t2 := time.Date(2025, 1, 1, 0, 0, 0, 0, time.UTC)
	if g := RecencyForSort(&Instance{LastPlayed: t2, CreatedAt: t1}); !g.Equal(t2) {
		t.Fatalf("later of two times: got %v want %v", g, t2)
	}
	if g := RecencyForSort(&Instance{LastPlayed: t1, CreatedAt: t2}); !g.Equal(t2) {
		t.Fatalf("later of two times when CreatedAt newer: got %v want %v", g, t2)
	}
	if g := RecencyForSort(&Instance{CreatedAt: t2}); !g.Equal(t2) {
		t.Fatalf("CreatedAt only: got %v", g)
	}
	if g := RecencyForSort(&Instance{LastPlayed: t2}); !g.Equal(t2) {
		t.Fatalf("LastPlayed only: got %v", g)
	}
}

func TestLaunchDownloadKey(t *testing.T) {
	if g, w := LaunchDownloadKey(&Instance{Version: "1.21.4", Loader: "vanilla"}), "1.21.4|vanilla|"; g != w {
		t.Fatalf("got %q want %q", g, w)
	}
	if g, w := LaunchDownloadKey(&Instance{Version: "1.21.4", Loader: "", LoaderVer: ""}), "1.21.4|vanilla|"; g != w {
		t.Fatalf("got %q want %q", g, w)
	}
	if g, w := LaunchDownloadKey(&Instance{Version: "1.21.4", Loader: "fabric", LoaderVer: "0.16.9"}), "1.21.4|fabric|0.16.9"; g != w {
		t.Fatalf("got %q want %q", g, w)
	}
}

func TestSanitizeInstanceDirName(t *testing.T) {
	cases := []struct{ in, want string }{
		{"My Fabric Pack", "My Fabric Pack"},
		{"a/b:c", "a-b-c"},
		{"weird/name:1", "weird-name-1"},
		{"  spaced  ", "spaced"},
		{"trailing... ", "trailing"},
		{"café 🎮 ok", "café 🎮 ok"},
		{"....", ""},
		{"", ""},
	}
	for _, tc := range cases {
		if got := SanitizeInstanceDirName(tc.in); got != tc.want {
			t.Fatalf("SanitizeInstanceDirName(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

func TestInstanceManager_CreateGeneratesUniqueID(t *testing.T) {
	tmp := t.TempDir()
	mgr := NewInstanceManager(tmp)

	a := &Instance{Name: "My Pack", Version: "1.21.4", Loader: "vanilla"}
	if err := mgr.Create(a); err != nil {
		t.Fatalf("Create a: %v", err)
	}
	if a.ID != "My Pack" {
		t.Fatalf("first ID = %q, want %q", a.ID, "My Pack")
	}

	// Same display name is allowed; folder/ID must be de-duplicated.
	b := &Instance{Name: "My Pack", Version: "1.21.4", Loader: "vanilla"}
	if err := mgr.Create(b); err != nil {
		t.Fatalf("Create b: %v", err)
	}
	if b.ID != "My Pack (2)" {
		t.Fatalf("second ID = %q, want %q", b.ID, "My Pack (2)")
	}
	if b.Name != "My Pack" {
		t.Fatalf("display name should be unchanged, got %q", b.Name)
	}

	for _, id := range []string{a.ID, b.ID} {
		if _, err := os.Stat(filepath.Join(tmp, "instances", id)); err != nil {
			t.Fatalf("folder for %q missing: %v", id, err)
		}
	}
}

func TestInstanceManager_CreateSanitizesFolderName(t *testing.T) {
	tmp := t.TempDir()
	mgr := NewInstanceManager(tmp)

	inst := &Instance{Name: "weird/name:1", Version: "1.21.4", Loader: "vanilla"}
	if err := mgr.Create(inst); err != nil {
		t.Fatalf("Create: %v", err)
	}
	if inst.ID != "weird-name-1" {
		t.Fatalf("ID = %q, want weird-name-1", inst.ID)
	}
	if inst.Name != "weird/name:1" {
		t.Fatalf("Name should be kept verbatim, got %q", inst.Name)
	}
}

func TestInstanceManager_CreateEmptyNameFallsBack(t *testing.T) {
	tmp := t.TempDir()
	mgr := NewInstanceManager(tmp)

	inst := &Instance{Name: "....", Version: "1.21.4", Loader: "vanilla"}
	if err := mgr.Create(inst); err != nil {
		t.Fatalf("Create: %v", err)
	}
	if inst.ID != "instance" {
		t.Fatalf("ID = %q, want instance", inst.ID)
	}
}

func TestInstanceManager_CreateHonorsExplicitID(t *testing.T) {
	tmp := t.TempDir()
	mgr := NewInstanceManager(tmp)

	inst := &Instance{ID: "explicit-id", Name: "Display Name", Version: "1.21.4", Loader: "vanilla"}
	if err := mgr.Create(inst); err != nil {
		t.Fatalf("Create: %v", err)
	}
	if inst.ID != "explicit-id" {
		t.Fatalf("ID = %q, want explicit-id (explicit IDs must be honored)", inst.ID)
	}
}

func TestInstanceManager_CreateAndLoad(t *testing.T) {
	// Setup temp directory
	tmpDir := t.TempDir()

	// Create manager
	mgr := NewInstanceManager(tmpDir)

	// Create instance
	inst := &Instance{
		ID:      "test-1",
		Name:    "Test Instance",
		Version: "1.21.4",
		Loader:  "vanilla",
	}

	if err := mgr.Create(inst); err != nil {
		t.Fatalf("Create failed: %v", err)
	}
	if inst.CreatedAt.IsZero() {
		t.Fatal("Create should set CreatedAt")
	}

	// Verify file exists
	configPath := filepath.Join(tmpDir, "instances", "test-1", "instance.json")
	if _, err := os.Stat(configPath); err != nil {
		t.Fatalf("Config file not created: %v", err)
	}

	// Load fresh
	mgr2 := NewInstanceManager(tmpDir)
	if err := mgr2.Load(); err != nil {
		t.Fatalf("Load failed: %v", err)
	}

	loaded, ok := mgr2.Get("test-1")
	if !ok {
		t.Fatal("Instance not found after reload")
	}

	if loaded.Name != "Test Instance" {
		t.Errorf("Name mismatch: got %q, want %q", loaded.Name, "Test Instance")
	}
	if loaded.Version != "1.21.4" {
		t.Errorf("Version mismatch: got %q, want %q", loaded.Version, "1.21.4")
	}
}

func TestInstanceManager_Delete(t *testing.T) {
	tmpDir := t.TempDir()
	mgr := NewInstanceManager(tmpDir)

	// Create instance
	inst := &Instance{
		ID:      "to-delete",
		Name:    "Delete Me",
		Version: "1.21.4",
		Loader:  "vanilla",
	}

	if err := mgr.Create(inst); err != nil {
		t.Fatalf("Create failed: %v", err)
	}

	// Verify it exists
	if _, ok := mgr.Get("to-delete"); !ok {
		t.Fatal("Instance should exist after creation")
	}

	// Delete it
	if err := mgr.Delete("to-delete"); err != nil {
		t.Fatalf("Delete failed: %v", err)
	}

	// Verify it's gone
	if _, ok := mgr.Get("to-delete"); ok {
		t.Error("Instance should not exist after deletion")
	}

	// Verify files are deleted
	instPath := filepath.Join(tmpDir, "instances", "to-delete")
	if _, err := os.Stat(instPath); !os.IsNotExist(err) {
		t.Error("Instance directory should be deleted")
	}
}

func TestInstanceManager_List(t *testing.T) {
	tmpDir := t.TempDir()
	mgr := NewInstanceManager(tmpDir)

	// Create multiple instances
	for i := 0; i < 3; i++ {
		inst := &Instance{
			ID:      "inst-" + string(rune('a'+i)),
			Name:    "Instance " + string(rune('A'+i)),
			Version: "1.21.4",
			Loader:  "vanilla",
		}
		if err := mgr.Create(inst); err != nil {
			t.Fatalf("Create failed: %v", err)
		}
	}

	list := mgr.List()
	if len(list) != 3 {
		t.Errorf("Expected 3 instances, got %d", len(list))
	}
}

func TestInstanceManager_UpdateLastPlayed(t *testing.T) {
	tmpDir := t.TempDir()
	mgr := NewInstanceManager(tmpDir)

	inst := &Instance{
		ID:      "play-test",
		Name:    "Play Test",
		Version: "1.21.4",
		Loader:  "vanilla",
	}

	if err := mgr.Create(inst); err != nil {
		t.Fatalf("Create failed: %v", err)
	}

	// Update last played
	before := time.Now()
	if err := mgr.UpdateLastPlayed("play-test"); err != nil {
		t.Fatalf("UpdateLastPlayed failed: %v", err)
	}
	after := time.Now()

	// Verify update
	updated, _ := mgr.Get("play-test")
	if updated.LastPlayed.Before(before) || updated.LastPlayed.After(after) {
		t.Error("LastPlayed should be between before and after")
	}

	// Reload and verify persistence
	mgr2 := NewInstanceManager(tmpDir)
	mgr2.Load()
	reloaded, _ := mgr2.Get("play-test")
	if reloaded.LastPlayed.IsZero() {
		t.Error("LastPlayed should persist after reload")
	}
}

func TestInstanceManager_AddPlayTime(t *testing.T) {
	tmpDir := t.TempDir()
	mgr := NewInstanceManager(tmpDir)

	inst := &Instance{ID: "pt", Name: "PT", Version: "1.21.4", Loader: "vanilla"}
	if err := mgr.Create(inst); err != nil {
		t.Fatalf("Create failed: %v", err)
	}

	// Accumulates across sessions.
	if err := mgr.AddPlayTime("pt", 120); err != nil {
		t.Fatalf("AddPlayTime failed: %v", err)
	}
	if err := mgr.AddPlayTime("pt", 60); err != nil {
		t.Fatalf("AddPlayTime failed: %v", err)
	}
	if got, _ := mgr.Get("pt"); got.PlayTime != 180 {
		t.Fatalf("PlayTime = %d, want 180", got.PlayTime)
	}

	// Non-positive deltas and unknown ids are no-ops.
	if err := mgr.AddPlayTime("pt", 0); err != nil {
		t.Fatalf("AddPlayTime(0) failed: %v", err)
	}
	if err := mgr.AddPlayTime("pt", -5); err != nil {
		t.Fatalf("AddPlayTime(-5) failed: %v", err)
	}
	if err := mgr.AddPlayTime("missing", 10); err != nil {
		t.Fatalf("AddPlayTime(missing) failed: %v", err)
	}
	if got, _ := mgr.Get("pt"); got.PlayTime != 180 {
		t.Fatalf("PlayTime = %d after no-ops, want 180", got.PlayTime)
	}

	// Persists across reload.
	mgr2 := NewInstanceManager(tmpDir)
	mgr2.Load()
	if reloaded, _ := mgr2.Get("pt"); reloaded.PlayTime != 180 {
		t.Fatalf("PlayTime = %d after reload, want 180", reloaded.PlayTime)
	}
}

func TestInstanceManager_EmptyDir(t *testing.T) {
	tmpDir := t.TempDir()
	mgr := NewInstanceManager(tmpDir)

	// Loading from non-existent directory should succeed
	if err := mgr.Load(); err != nil {
		t.Fatalf("Load from empty dir failed: %v", err)
	}

	// Should have no instances
	if len(mgr.List()) != 0 {
		t.Error("Expected empty list from new directory")
	}
}

// A running launch holds the *Instance it started with and persists through the
// manager. A reload mid-game must not swap in a fresh copy, or the launch's
// final save (stale copy) overwrites the playtime credited at exit.
func TestInstanceManager_LoadKeepsExistingPointers(t *testing.T) {
	mgr := NewInstanceManager(t.TempDir())
	inst := &Instance{ID: "a", Name: "A", Version: "1.21.4"}
	if err := mgr.Create(inst); err != nil {
		t.Fatal(err)
	}

	if err := mgr.Load(); err != nil {
		t.Fatal(err)
	}
	if got, _ := mgr.Get("a"); got != inst {
		t.Fatal("Load replaced a known instance's pointer")
	}

	// Simulate game exit: credit playtime by ID, then the launch's final save.
	if err := mgr.AddPlayTime("a", 90); err != nil {
		t.Fatal(err)
	}
	inst.IsFullyDownloaded = true
	if err := mgr.Update(inst); err != nil {
		t.Fatal(err)
	}

	fresh := NewInstanceManager(mgr.basePath)
	if err := fresh.Load(); err != nil {
		t.Fatal(err)
	}
	got, _ := fresh.Get("a")
	if got.PlayTime != 90 || !got.IsFullyDownloaded {
		t.Errorf("on disk: PlayTime=%d IsFullyDownloaded=%v, want 90/true", got.PlayTime, got.IsFullyDownloaded)
	}
}

// Instances whose folder was removed outside the app disappear on reload;
// new folders appear.
func TestInstanceManager_LoadSyncsWithDisk(t *testing.T) {
	dir := t.TempDir()
	mgr := NewInstanceManager(dir)
	for _, id := range []string{"keep", "gone"} {
		if err := mgr.Create(&Instance{ID: id, Name: id}); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.RemoveAll(filepath.Join(dir, "instances", "gone")); err != nil {
		t.Fatal(err)
	}
	other := NewInstanceManager(dir)
	if err := other.Create(&Instance{ID: "new", Name: "new"}); err != nil {
		t.Fatal(err)
	}

	if err := mgr.Load(); err != nil {
		t.Fatal(err)
	}
	for id, want := range map[string]bool{"keep": true, "gone": false, "new": true} {
		if _, ok := mgr.Get(id); ok != want {
			t.Errorf("Get(%q) present=%v, want %v", id, ok, want)
		}
	}
}

// The event loop (Load/List via Cmds) and a running launch (AddPlayTime,
// UpdateLastPlayed, Update) hit the manager concurrently; unguarded, that was a
// fatal "concurrent map read and map write". Run with -race.
func TestInstanceManager_ConcurrentAccess(t *testing.T) {
	mgr := NewInstanceManager(t.TempDir())
	for i := range 4 {
		if err := mgr.Create(&Instance{ID: fmt.Sprintf("i%d", i), Name: "x"}); err != nil {
			t.Fatal(err)
		}
	}
	var wg sync.WaitGroup
	for w := range 4 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			id := fmt.Sprintf("i%d", w)
			for range 50 {
				switch w {
				case 0:
					_ = mgr.Load()
				case 1:
					_ = mgr.List()
				case 2:
					_ = mgr.AddPlayTime(id, 1)
				case 3:
					_ = mgr.UpdateLastPlayed(id)
				}
			}
		}()
	}
	wg.Wait()
}

// Hand-editing instance.json (the only way to change version/loader) must be
// picked up on reload — and not reverted by the next save — while the
// in-memory pointer stays the same. Changes mctui made itself are kept.
func TestInstanceManager_LoadPicksUpHandEdits(t *testing.T) {
	mgr := NewInstanceManager(t.TempDir())
	inst := &Instance{ID: "a", Name: "A", Version: "1.21.4"}
	if err := mgr.Create(inst); err != nil {
		t.Fatal(err)
	}

	// Unsaved in-memory state survives a reload when the file didn't change.
	inst.IsFullyDownloaded = true
	if err := mgr.Load(); err != nil {
		t.Fatal(err)
	}
	if !inst.IsFullyDownloaded {
		t.Fatal("a reload discarded in-memory state although the file was unchanged")
	}

	// A hand edit is taken into the same pointer.
	path := filepath.Join(inst.Path, "instance.json")
	edited := `{"id": "a", "name": "A", "version": "1.20.1", "path": ` + fmt.Sprintf("%q", inst.Path) + `}`
	if err := os.WriteFile(path, []byte(edited), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := mgr.Load(); err != nil {
		t.Fatal(err)
	}
	got, _ := mgr.Get("a")
	if got != inst {
		t.Fatal("reload replaced the pointer")
	}
	if inst.Version != "1.20.1" {
		t.Errorf("Version = %q, want the hand-edited 1.20.1", inst.Version)
	}

	// …and the next save keeps it.
	if err := mgr.AddPlayTime("a", 60); err != nil {
		t.Fatal(err)
	}
	fresh := NewInstanceManager(mgr.basePath)
	if err := fresh.Load(); err != nil {
		t.Fatal(err)
	}
	if g, _ := fresh.Get("a"); g.Version != "1.20.1" || g.PlayTime != 60 {
		t.Errorf("on disk: Version=%q PlayTime=%d, want 1.20.1 / 60", g.Version, g.PlayTime)
	}
}

// Removing the whole instances folder while mctui runs empties the list.
func TestInstanceManager_LoadWithInstancesDirGone(t *testing.T) {
	dir := t.TempDir()
	mgr := NewInstanceManager(dir)
	if err := mgr.Create(&Instance{ID: "a", Name: "A"}); err != nil {
		t.Fatal(err)
	}
	if err := os.RemoveAll(filepath.Join(dir, "instances")); err != nil {
		t.Fatal(err)
	}
	if err := mgr.Load(); err != nil {
		t.Fatal(err)
	}
	if n := len(mgr.List()); n != 0 {
		t.Errorf("%d instances listed after their folder was removed", n)
	}
}
