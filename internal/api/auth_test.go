package api

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
)

// newTestAuthClient builds an AuthClient starting from the production endpoints
// and applies override to point selected URLs at httptest servers. Because the
// endpoints live on the instance (not package vars), each test is isolated and
// safe under t.Parallel.
func newTestAuthClient(clientID string, override func(*authEndpoints)) *AuthClient {
	c := NewAuthClient(clientID)
	ep := c.endpoints
	override(&ep)
	c.endpoints = ep
	return c
}

func TestAuthClient_RequestDeviceCode(t *testing.T) {
	t.Parallel()
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

	client := newTestAuthClient("test-client", func(ep *authEndpoints) {
		ep.msaDeviceCode = ts.URL
	})
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
	t.Parallel()
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

	client := newTestAuthClient("test-client", func(ep *authEndpoints) {
		ep.msaToken = ts.URL
	})
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

func TestAuthClient_FetchProfile_unauthorized(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer bad" {
			t.Errorf("Authorization header: %q", r.Header.Get("Authorization"))
		}
		w.WriteHeader(http.StatusUnauthorized)
	}))
	defer ts.Close()

	client := newTestAuthClient("dummy", func(ep *authEndpoints) {
		ep.mcProfile = ts.URL
	})
	_, err := client.FetchProfile(context.Background(), "bad")
	if !errors.Is(err, ErrMinecraftSessionInvalid) {
		t.Fatalf("err = %v, want ErrMinecraftSessionInvalid", err)
	}
}

func TestAuthClient_RefreshMSAToken(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "POST" {
			t.Errorf("Expected POST, got %s", r.Method)
		}
		if r.FormValue("grant_type") != "refresh_token" {
			t.Errorf("grant_type = %q, want refresh_token", r.FormValue("grant_type"))
		}
		if r.FormValue("refresh_token") != "old_refresh" {
			t.Errorf("refresh_token = %q, want old_refresh", r.FormValue("refresh_token"))
		}
		if r.FormValue("client_id") != "test-client" {
			t.Errorf("client_id = %q, want test-client", r.FormValue("client_id"))
		}
		w.Header().Set("Content-Type", "application/json")
		// Microsoft rotates the refresh token: the response carries a NEW one.
		json.NewEncoder(w).Encode(MSATokenResponse{
			AccessToken:  "new_msa_access",
			RefreshToken: "rotated_refresh",
			ExpiresIn:    3600,
		})
	}))
	defer ts.Close()

	client := newTestAuthClient("test-client", func(ep *authEndpoints) {
		ep.msaToken = ts.URL
	})
	resp, err := client.RefreshMSAToken(context.Background(), "old_refresh")
	if err != nil {
		t.Fatalf("Failed: %v", err)
	}
	if resp.AccessToken != "new_msa_access" {
		t.Errorf("AccessToken = %q, want new_msa_access", resp.AccessToken)
	}
	if resp.RefreshToken != "rotated_refresh" {
		t.Errorf("RefreshToken = %q, want rotated_refresh (rotated token must be returned)", resp.RefreshToken)
	}
}

func TestAuthClient_RefreshMSAToken_errors(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name        string
		status      int
		wantAuthErr bool // expect ErrMSARefreshInvalid
	}{
		{name: "auth error (invalid_grant)", status: http.StatusBadRequest, wantAuthErr: true},
		{name: "unauthorized", status: http.StatusUnauthorized, wantAuthErr: true},
		{name: "server error is not an auth error", status: http.StatusInternalServerError, wantAuthErr: false},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(tc.status)
				_, _ = w.Write([]byte(`{"error":"invalid_grant"}`))
			}))
			defer ts.Close()

			client := newTestAuthClient("test-client", func(ep *authEndpoints) {
				ep.msaToken = ts.URL
			})
			_, err := client.RefreshMSAToken(context.Background(), "old_refresh")
			if err == nil {
				t.Fatal("expected error, got nil")
			}
			if got := errors.Is(err, ErrMSARefreshInvalid); got != tc.wantAuthErr {
				t.Errorf("errors.Is(err, ErrMSARefreshInvalid) = %v, want %v (err=%v)", got, tc.wantAuthErr, err)
			}
		})
	}
}

func TestAuthClient_RefreshSession(t *testing.T) {
	t.Parallel()
	// MSA token endpoint: returns rotated refresh token + MSA access token.
	tokenSrv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(MSATokenResponse{
			AccessToken:  "new_msa_access",
			RefreshToken: "rotated_refresh",
			ExpiresIn:    3600,
		})
	}))
	defer tokenSrv.Close()

	// Xbox + XSTS share the doXboxRequest path; return a token + user hash.
	xboxSrv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		resp := XboxAuthResponse{Token: "xbl_or_xsts"}
		resp.DisplayClaims.XUI = []struct {
			UHS string `json:"uhs"`
		}{{UHS: "userhash"}}
		json.NewEncoder(w).Encode(resp)
	}))
	defer xboxSrv.Close()

	// Minecraft login endpoint: returns the final Minecraft access token.
	mcSrv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(MinecraftAuthResponse{
			AccessToken: "mc_access_token",
			ExpiresIn:   86400,
		})
	}))
	defer mcSrv.Close()

	client := newTestAuthClient("test-client", func(ep *authEndpoints) {
		ep.msaToken = tokenSrv.URL
		ep.xboxUserAuth = xboxSrv.URL
		ep.xstsAuth = xboxSrv.URL
		ep.mcAuth = mcSrv.URL
	})
	mcToken, newRefresh, expiresIn, err := client.RefreshSession(context.Background(), "old_refresh")
	if err != nil {
		t.Fatalf("Failed: %v", err)
	}
	if mcToken != "mc_access_token" {
		t.Errorf("mcToken = %q, want mc_access_token", mcToken)
	}
	if newRefresh != "rotated_refresh" {
		t.Errorf("newRefresh = %q, want rotated_refresh (must persist the rotated token)", newRefresh)
	}
	if expiresIn != 86400 {
		t.Errorf("expiresIn = %d, want 86400", expiresIn)
	}
}

func TestAuthClient_ValidateMinecraftToken_ok(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(MinecraftProfile{ID: "u", Name: "n"})
	}))
	defer ts.Close()

	client := newTestAuthClient("dummy", func(ep *authEndpoints) {
		ep.mcProfile = ts.URL
	})
	if err := client.ValidateMinecraftToken(context.Background(), "token"); err != nil {
		t.Fatal(err)
	}
}
