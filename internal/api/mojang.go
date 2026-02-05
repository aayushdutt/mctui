// Package api contains HTTP clients for external services.
// Each API client is self-contained and handles its own caching.
package api

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"time"

	"github.com/quasar/mctui/internal/core"
)

const (
	mojangVersionManifestURL = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
)

// MojangClient handles Mojang API interactions
type MojangClient struct {
	httpClient       *http.Client
	manifest         *core.VersionManifest
	manifestFetched  time.Time
	manifestTTL      time.Duration
	versionCacheRoot string
}

// NewMojangClient creates a new Mojang API client
func NewMojangClient(dataDir string) *MojangClient {
	return &MojangClient{
		httpClient: &http.Client{
			Timeout: 30 * time.Second,
		},
		manifestTTL:      5 * time.Minute,
		versionCacheRoot: filepath.Join(dataDir, "cache", "versions"),
	}
}

// GetVersionManifest fetches the version manifest from Mojang
func (c *MojangClient) GetVersionManifest(ctx context.Context) (*core.VersionManifest, error) {
	// Check cache
	if c.manifest != nil && time.Since(c.manifestFetched) < c.manifestTTL {
		return c.manifest, nil
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
	c.manifest = &manifest
	c.manifestFetched = time.Now()

	return &manifest, nil
}

// GetVersionDetails fetches detailed version information
func (c *MojangClient) GetVersionDetails(ctx context.Context, version *core.Version) (*core.VersionDetails, error) {
	// Fetch from network
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

// ResolveVersionDetails resolves version details with a minimal disk cache.
// If offline is true, it only reads from disk.
func (c *MojangClient) ResolveVersionDetails(ctx context.Context, versionID string, offline bool) (*core.VersionDetails, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	if offline {
		return c.loadVersionDetails(versionID)
	}

	version, err := c.FindVersion(ctx, versionID)
	if err != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, err
	}

	details, err := c.GetVersionDetails(ctx, version)
	if err != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, err
	}

	_ = c.saveVersionDetails(versionID, details)

	return details, nil
}

func (c *MojangClient) loadVersionDetails(versionID string) (*core.VersionDetails, error) {
	path := c.versionDetailsPath(versionID)
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	var details core.VersionDetails
	if err := json.Unmarshal(data, &details); err != nil {
		return nil, fmt.Errorf("decoding cached version details: %w", err)
	}

	return &details, nil
}

func (c *MojangClient) saveVersionDetails(versionID string, details *core.VersionDetails) error {
	if details == nil {
		return nil
	}

	if err := os.MkdirAll(c.versionCacheRoot, 0o755); err != nil {
		return err
	}

	path := c.versionDetailsPath(versionID)
	data, err := json.Marshal(details)
	if err != nil {
		return fmt.Errorf("encoding version details: %w", err)
	}

	return os.WriteFile(path, data, 0o644)
}

func (c *MojangClient) versionDetailsPath(versionID string) string {
	return filepath.Join(c.versionCacheRoot, fmt.Sprintf("%s.json", versionID))
}
