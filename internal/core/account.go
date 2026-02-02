package core

import (
	"time"
)

// AccountType represents the type of account
type AccountType string

const (
	AccountTypeMSA     AccountType = "msa"
	AccountTypeOffline AccountType = "offline"
)

// Account represents a Minecraft account
type Account struct {
	ID             string      `json:"id"`             // UUID
	Name           string      `json:"name"`           // Username
	Type           AccountType `json:"type"`           // msa or offline
	AccessToken    string      `json:"accessToken"`    // Valid Minecraft Access Token
	ExpiresAt      time.Time   `json:"expiresAt"`      // When MC token expires
	MSARefreshToken string     `json:"msaRefreshToken,omitempty"` // For refreshing MSA token
}

// IsExpired checks if the token is expired (with 5m buffer)
func (a *Account) IsExpired() bool {
	if a.Type == AccountTypeOffline {
		return false
	}
	return time.Now().Add(5 * time.Minute).After(a.ExpiresAt)
}
