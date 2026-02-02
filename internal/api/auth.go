// Package api MSA (Microsoft Authentication) client.
package api

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"time"
)

const (
	msaDeviceCodeURL = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode"
	msaTokenURL      = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
	xboxAuthURL      = "https://user.auth.xboxlive.com/user/authenticate"
	xstsAuthURL      = "https://xsts.auth.xboxlive.com/xsts/authorize"
	mcAuthURL        = "https://api.minecraftservices.com/authentication/login_with_xbox"
	mcProfileURL     = "https://api.minecraftservices.com/minecraft/profile"
)

// AuthClient handles Microsoft/Xbox/Minecraft authentication
type AuthClient struct {
	httpClient *http.Client
	clientID   string
}

func NewAuthClient(clientID string) *AuthClient {
	return &AuthClient{
		httpClient: &http.Client{Timeout: 30 * time.Second},
		clientID:   clientID,
	}
}

type DeviceCodeResponse struct {
	DeviceCode      string `json:"device_code"`
	UserCode        string `json:"user_code"`
	VerificationURI string `json:"verification_uri"`
	ExpiresIn       int    `json:"expires_in"`
	Interval        int    `json:"interval"`
	Message         string `json:"message"`
}

type MSATokenResponse struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
	ExpiresIn    int    `json:"expires_in"`
}

type XboxAuthResponse struct {
	Token         string `json:"Token"`
	DisplayClaims struct {
		XUI []struct {
			UHS string `json:"uhs"`
		} `json:"xui"`
	} `json:"DisplayClaims"`
}

type MinecraftAuthResponse struct {
	AccessToken string `json:"access_token"`
	ExpiresIn   int    `json:"expires_in"`
}

type MinecraftProfile struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}

func (c *AuthClient) RequestDeviceCode(ctx context.Context) (*DeviceCodeResponse, error) {
	data := url.Values{"client_id": {c.clientID}, "scope": {"XboxLive.signin offline_access"}}
	req, _ := http.NewRequestWithContext(ctx, "POST", msaDeviceCodeURL, bytes.NewBufferString(data.Encode()))
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var result DeviceCodeResponse
	json.NewDecoder(resp.Body).Decode(&result)
	return &result, nil
}

func (c *AuthClient) PollForToken(ctx context.Context, dc *DeviceCodeResponse) (*MSATokenResponse, error) {
	data := url.Values{
		"client_id": {c.clientID}, "grant_type": {"urn:ietf:params:oauth:grant-type:device_code"},
		"device_code": {dc.DeviceCode},
	}
	interval := time.Duration(dc.Interval) * time.Second
	deadline := time.Now().Add(time.Duration(dc.ExpiresIn) * time.Second)
	for time.Now().Before(deadline) {
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(interval):
		}
		req, _ := http.NewRequestWithContext(ctx, "POST", msaTokenURL, bytes.NewBufferString(data.Encode()))
		req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
		resp, err := c.httpClient.Do(req)
		if err != nil {
			continue
		}
		var result struct {
			MSATokenResponse
			Error string `json:"error"`
		}
		json.NewDecoder(resp.Body).Decode(&result)
		resp.Body.Close()
		if result.Error == "" {
			return &result.MSATokenResponse, nil
		}
		if result.Error == "authorization_pending" {
			continue
		}
		return nil, fmt.Errorf("auth error: %s", result.Error)
	}
	return nil, fmt.Errorf("timeout")
}
