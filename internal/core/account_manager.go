package core

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// AccountManager handles account storage
type AccountManager struct {
	Accounts []*Account `json:"accounts"`
	ActiveID string     `json:"activeId"` // ID of the active account
	filePath string
}

// NewAccountManager creates a new manager
func NewAccountManager(dataDir string) *AccountManager {
	return &AccountManager{
		Accounts: []*Account{},
		filePath: filepath.Join(dataDir, "accounts.json"),
	}
}

// Load reads accounts from disk
func (m *AccountManager) Load() error {
	data, err := os.ReadFile(m.filePath)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	return json.Unmarshal(data, m)
}

// Save writes accounts to disk
func (m *AccountManager) Save() error {
	data, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(m.filePath, data, 0644)
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
