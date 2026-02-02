package core

import (
	"os"
	"testing"
	"time"
)

func TestAccountManager_LoadSave(t *testing.T) {
	// Setup temp dir
	tmpDir, err := os.MkdirTemp("", "mctui_auth_test")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpDir)

	manager := NewAccountManager(tmpDir)

	// Create a dummy account
	acc := &Account{
		ID:          "acc1",
		Name:        "TestPlayer",
		Type:        AccountTypeMSA,
		AccessToken: "token123",
		ExpiresAt:   time.Now().Add(1 * time.Hour),
	}

	// Add and Save
	manager.Add(acc)
	if err := manager.Save(); err != nil {
		t.Fatalf("Save failed: %v", err)
	}

	// Load new manager
	manager2 := NewAccountManager(tmpDir)
	if err := manager2.Load(); err != nil {
		t.Fatalf("Load failed: %v", err)
	}

	// Verify
	if len(manager2.Accounts) != 1 {
		t.Errorf("Expected 1 account, got %d", len(manager2.Accounts))
	}
	if manager2.Accounts[0].Name != "TestPlayer" {
		t.Errorf("Expected name TestPlayer, got %s", manager2.Accounts[0].Name)
	}
	if manager2.ActiveID != "acc1" {
		t.Errorf("Expected active ID acc1, got %s", manager2.ActiveID)
	}
}

func TestAccountManager_SetActive(t *testing.T) {
	tmpDir, _ := os.MkdirTemp("", "mctui_auth_test")
	defer os.RemoveAll(tmpDir)
	manager := NewAccountManager(tmpDir)

	manager.Add(&Account{ID: "1", Name: "A"})
	manager.Add(&Account{ID: "2", Name: "B"})

	// Default first one active
	if manager.ActiveID != "1" {
		t.Errorf("Expected default active 1, got %s", manager.ActiveID)
	}

	// Switch
	if err := manager.SetActive("2"); err != nil {
		t.Errorf("SetActive failed: %v", err)
	}
	if manager.ActiveID != "2" {
		t.Errorf("Expected active 2, got %s", manager.ActiveID)
	}

	// Fail
	if err := manager.SetActive("3"); err == nil {
		t.Error("Expected error for missing account, got nil")
	}
}
