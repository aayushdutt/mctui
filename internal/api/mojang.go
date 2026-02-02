// Package api contains HTTP clients for external services.
// Each API client is self-contained and handles its own caching.
package api

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"github.com/quasar/mctui/internal/core"
)

const (
	mojangVersionManifestURL = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
)

// MojangClient handles Mojang API interactions
type MojangClient struct {
	httpClient *http.Client
	cache      *versionCache
}

// versionCache caches version manifest to avoid repeated fetches
type versionCache struct {
	manifest  *core.VersionManifest
	fetchedAt time.Time
	ttl       time.Duration
}

// NewMojangClient creates a new Mojang API client
func NewMojangClient() *MojangClient {
	return &MojangClient{
		httpClient: &http.Client{
			Timeout: 30 * time.Second,
		},
		cache: &versionCache{
			ttl: 5 * time.Minute,
		},
	}
}

// GetVersionManifest fetches the version manifest from Mojang
func (c *MojangClient) GetVersionManifest(ctx context.Context) (*core.VersionManifest, error) {
	// Check cache
	if c.cache.manifest != nil && time.Since(c.cache.fetchedAt) < c.cache.ttl {
		return c.cache.manifest, nil
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, mojangVersionManifestURL, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("fetching manifest: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	var manifest core.VersionManifest
	if err := json.NewDecoder(resp.Body).Decode(&manifest); err != nil {
		return nil, fmt.Errorf("decoding manifest: %w", err)
	}

	// Update cache
	c.cache.manifest = &manifest
	c.cache.fetchedAt = time.Now()

	return &manifest, nil
}

// GetVersionDetails fetches detailed version information
func (c *MojangClient) GetVersionDetails(ctx context.Context, version *core.Version) (*core.VersionDetails, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, version.URL, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("fetching version details: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	var details core.VersionDetails
	if err := json.NewDecoder(resp.Body).Decode(&details); err != nil {
		return nil, fmt.Errorf("decoding version details: %w", err)
	}

	return &details, nil
}

// GetLatestRelease returns the latest release version ID
func (c *MojangClient) GetLatestRelease(ctx context.Context) (string, error) {
	manifest, err := c.GetVersionManifest(ctx)
	if err != nil {
		return "", err
	}
	return manifest.Latest.Release, nil
}

// GetLatestSnapshot returns the latest snapshot version ID
func (c *MojangClient) GetLatestSnapshot(ctx context.Context) (string, error) {
	manifest, err := c.GetVersionManifest(ctx)
	if err != nil {
		return "", err
	}
	return manifest.Latest.Snapshot, nil
}

// FindVersion finds a version by ID in the manifest
func (c *MojangClient) FindVersion(ctx context.Context, id string) (*core.Version, error) {
	manifest, err := c.GetVersionManifest(ctx)
	if err != nil {
		return nil, err
	}

	for _, v := range manifest.Versions {
		if v.ID == id {
			return &v, nil
		}
	}

	return nil, fmt.Errorf("version not found: %s", id)
}
