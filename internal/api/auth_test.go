package api

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestAuthClient_RequestDeviceCode(t *testing.T) {
	// Mock Server
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "POST" {
			t.Errorf("Expected POST, got %s", r.Method)
		}
		if r.FormValue("client_id") != "test-client" {
			t.Errorf("Expected client_id=test-client, got %s", r.FormValue("client_id"))
		}
		
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(DeviceCodeResponse{
			DeviceCode:      "CODE123",
			UserCode:        "ABCD",
			VerificationURI: "http://link",
			ExpiresIn:       900,
			Interval:        5,
		})
	}))
	defer ts.Close()

	// Override URL
	oldURL := msaDeviceCodeURL
	msaDeviceCodeURL = ts.URL
	defer func() { msaDeviceCodeURL = oldURL }()

	// Test
	client := NewAuthClient("test-client")
	resp, err := client.RequestDeviceCode(context.Background())
	if err != nil {
		t.Fatalf("Failed: %v", err)
	}

	if resp.DeviceCode != "CODE123" {
		t.Errorf("Got %s, want CODE123", resp.DeviceCode)
	}
	if resp.UserCode != "ABCD" {
		t.Errorf("Got %s, want ABCD", resp.UserCode)
	}
}

func TestAuthClient_PollForToken(t *testing.T) {
	attempts := 0
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts++
		w.Header().Set("Content-Type", "application/json")
		
		if attempts == 1 {
			// First attempt: pending
			json.NewEncoder(w).Encode(map[string]string{
				"error": "authorization_pending",
			})
			return
		}

		// Second attempt: success
		json.NewEncoder(w).Encode(MSATokenResponse{
			AccessToken:  "access_token_123",
			RefreshToken: "refresh_token_123",
			ExpiresIn:    3600,
		})
	}))
	defer ts.Close()

	oldURL := msaTokenURL
	msaTokenURL = ts.URL
	defer func() { msaTokenURL = oldURL }()

	client := NewAuthClient("test-client")
	dc := &DeviceCodeResponse{
		DeviceCode: "CODE123",
		Interval:   0, // Instant retry for test speed
		ExpiresIn:  10,
	}

	resp, err := client.PollForToken(context.Background(), dc)
	if err != nil {
		t.Fatalf("Failed: %v", err)
	}

	if resp.AccessToken != "access_token_123" {
		t.Errorf("Got %s, want access_token_123", resp.AccessToken)
	}
	if attempts != 2 {
		t.Errorf("Expected 2 attempts, got %d", attempts)
	}
}
