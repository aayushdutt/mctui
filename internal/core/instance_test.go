package core

import (
	"os"
	"path/filepath"
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
