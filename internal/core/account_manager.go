package core

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/aayushdutt/mctui/internal/fsutil"
)

// AccountManager handles account storage
type AccountManager struct {
	Accounts []*Account `json:"accounts"`
	ActiveID string     `json:"activeId"` // ID of the active account
	filePath string

	// saveBlocked, when set, makes Save refuse: accounts.json exists but
	// couldn't be read, so writing over it could destroy the tokens it holds.
	saveBlocked error
}

// BlockSaves makes Save refuse with reason (see saveBlocked).
func (m *AccountManager) BlockSaves(reason error) {
	m.saveBlocked = reason
}

// NewAccountManager creates a new manager
func NewAccountManager(dataDir string) *AccountManager {
	return &AccountManager{
		Accounts: []*Account{},
		filePath: filepath.Join(dataDir, "accounts.json"),
	}
}

// accountsFilePerm keeps accounts.json owner-only: it holds the Minecraft access
// token and the long-lived Microsoft refresh token.
const accountsFilePerm = 0o600

// Load reads accounts from disk
func (m *AccountManager) Load() error {
	data, err := os.ReadFile(m.filePath)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	if err := json.Unmarshal(data, m); err != nil {
		return err
	}
	// Tighten files written by older versions (0644). Best effort: a failure
	// here must not lose the accounts we just read.
	_ = os.Chmod(m.filePath, accountsFilePerm)
	return nil
}

// Save writes accounts to disk atomically, readable only by the owner.
func (m *AccountManager) Save() error {
	if m.saveBlocked != nil {
		return fmt.Errorf("not overwriting accounts.json, which couldn't be read: %w", m.saveBlocked)
	}
	data, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return err
	}
	return fsutil.WriteFileAtomic(m.filePath, data, accountsFilePerm)
}

// Add adds or updates an account
func (m *AccountManager) Add(acc *Account) {
	// Check if exists
	for i, a := range m.Accounts {
		if a.ID == acc.ID {
			m.Accounts[i] = acc
			return
		}
	}
	m.Accounts = append(m.Accounts, acc)
	if m.ActiveID == "" {
		m.ActiveID = acc.ID
	}
}

// GetActive returns the currently active account
func (m *AccountManager) GetActive() *Account {
	if m.ActiveID == "" {
		return nil
	}
	for _, a := range m.Accounts {
		if a.ID == m.ActiveID {
			return a
		}
	}
	return nil
}

// SetActive sets the active account
func (m *AccountManager) SetActive(id string) error {
	for _, a := range m.Accounts {
		if a.ID == id {
			m.ActiveID = id
			return nil
		}
	}
	return fmt.Errorf("account not found: %s", id)
}
