// Package api MSA (Microsoft Authentication) client.
package api

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"time"
)

var (
	msaDeviceCodeURL = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode"
	msaTokenURL      = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
	xboxUserAuthURL  = "https://user.auth.xboxlive.com/user/authenticate"
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

type XboxAuthRequest struct {
	Properties   XboxAuthProperties `json:"Properties"`
	RelyingParty string             `json:"RelyingParty"`
	TokenType    string             `json:"TokenType"`
}

type XboxAuthProperties struct {
	AuthMethod string   `json:"AuthMethod,omitempty"`
	SiteName   string   `json:"SiteName,omitempty"`
	RpsTicket  string   `json:"RpsTicket,omitempty"`
	SandboxId  string   `json:"SandboxId,omitempty"`
	UserTokens []string `json:"UserTokens,omitempty"`
}

type XboxAuthResponse struct {
	Token         string `json:"Token"`
	DisplayClaims struct {
		XUI []struct {
			UHS string `json:"uhs"`
		} `json:"xui"`
	} `json:"DisplayClaims"`
}

type MinecraftAuthRequest struct {
	IdentityToken string `json:"identityToken"`
}

type MinecraftAuthResponse struct {
	AccessToken string `json:"access_token"`
	ExpiresIn   int    `json:"expires_in"`
}

type MinecraftProfile struct {
	ID    string `json:"id"`
	Name  string `json:"name"`
	Skins []struct {
		ID      string `json:"id"`
		State   string `json:"state"`
		URL     string `json:"url"`
		Variant string `json:"variant"`
	} `json:"skins"`
}

// RequestDeviceCode initiates the device code flow
func (c *AuthClient) RequestDeviceCode(ctx context.Context) (*DeviceCodeResponse, error) {
	data := url.Values{
		"client_id": {c.clientID},
		"scope":     {"XboxLive.signin offline_access"},
	}
	req, _ := http.NewRequestWithContext(ctx, "POST", msaDeviceCodeURL, bytes.NewBufferString(data.Encode()))
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("device code request failed: %s", string(body))
	}

	var result DeviceCodeResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	return &result, nil
}

// PollForToken polls Microsoft for the token after user authorizes
func (c *AuthClient) PollForToken(ctx context.Context, dc *DeviceCodeResponse) (*MSATokenResponse, error) {
	data := url.Values{
		"client_id":   {c.clientID},
		"grant_type":  {"urn:ietf:params:oauth:grant-type:device_code"},
		"device_code": {dc.DeviceCode},
	}
	interval := time.Duration(dc.Interval) * time.Second
	if interval == 0 {
		interval = 5 * time.Second
	}
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
			continue // Network error, retry
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
		if result.Error == "slow_down" {
			interval += 5 * time.Second
			continue
		}
		return nil, fmt.Errorf("auth error: %s", result.Error)
	}
	return nil, fmt.Errorf("timeout waiting for user authorization")
}

// AuthenticateXbox exchanges MSA Access Token for Xbox Live Token
func (c *AuthClient) AuthenticateXbox(ctx context.Context, msaAccessToken string) (*XboxAuthResponse, error) {
	reqBody := XboxAuthRequest{
		Properties: XboxAuthProperties{
			AuthMethod: "RPS",
			SiteName:   "user.auth.xboxlive.com",
			RpsTicket:  "d=" + msaAccessToken,
		},
		RelyingParty: "http://auth.xboxlive.com",
		TokenType:    "JWT",
	}
	return c.doXboxRequest(ctx, xboxUserAuthURL, reqBody)
}

// AuthenticateXSTS exchanges Xbox Live Token for XSTS Token
func (c *AuthClient) AuthenticateXSTS(ctx context.Context, xboxToken string) (*XboxAuthResponse, error) {
	reqBody := XboxAuthRequest{
		Properties: XboxAuthProperties{
			SandboxId:  "RETAIL",
			UserTokens: []string{xboxToken},
		},
		RelyingParty: "rp://api.minecraftservices.com/",
		TokenType:    "JWT",
	}
	return c.doXboxRequest(ctx, xstsAuthURL, reqBody)
}

func (c *AuthClient) doXboxRequest(ctx context.Context, url string, body XboxAuthRequest) (*XboxAuthResponse, error) {
	jsonBody, _ := json.Marshal(body)
	req, _ := http.NewRequestWithContext(ctx, "POST", url, bytes.NewBuffer(jsonBody))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	req.Header.Set("x-xbl-contract-version", "1")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		// Try to parse error
		respBody, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("xbox auth failed (%d): %s", resp.StatusCode, string(respBody))
	}

	var result XboxAuthResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	return &result, nil
}

// LoginWithXbox exchanges XSTS Token and UHS for Minecraft Access Token
func (c *AuthClient) LoginWithXbox(ctx context.Context, uhs, xstsToken string) (*MinecraftAuthResponse, error) {
	reqBody := MinecraftAuthRequest{
		IdentityToken: fmt.Sprintf("XBL3.0 x=%s;%s", uhs, xstsToken),
	}
	jsonBody, _ := json.Marshal(reqBody)

	req, _ := http.NewRequestWithContext(ctx, "POST", mcAuthURL, bytes.NewBuffer(jsonBody))
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("minecraft login failed (%d): %s", resp.StatusCode, string(respBody))
	}

	var result MinecraftAuthResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	return &result, nil
}

// FetchProfile gets the Minecraft profile (uuid, name, skins)
func (c *AuthClient) FetchProfile(ctx context.Context, accessToken string) (*MinecraftProfile, error) {
	req, _ := http.NewRequestWithContext(ctx, "GET", mcProfileURL, nil)
	req.Header.Set("Authorization", "Bearer "+accessToken)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("fetch profile failed: %d", resp.StatusCode)
	}

	var result MinecraftProfile
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	return &result, nil
}
