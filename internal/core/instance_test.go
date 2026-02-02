package core

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

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
