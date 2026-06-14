// Package mods orchestrates Modrinth discovery and installs into instance folders.
// UI and HTTP types stay in internal/api; this layer encodes launcher-specific rules (Fabric + game version).
package mods

import (
	"context"
	"crypto/sha1"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/download"
)

// ModrinthAPI is the subset of *api.ModrinthClient the mods layer depends on.
// Declared here (consumer-side) so the dependency resolver can be unit-tested with a fake.
// *api.ModrinthClient satisfies this interface.
type ModrinthAPI interface {
	Search(ctx context.Context, opts api.SearchOptions) (*api.SearchResult, error)
	GetProject(ctx context.Context, idOrSlug string) (*api.Project, error)
	GetProjectVersions(ctx context.Context, projectID string, loaders []string, gameVersions []string) ([]api.ProjectVersion, error)
	GetVersion(ctx context.Context, versionID string) (*api.ProjectVersion, error)
}

// Service wires Modrinth API calls to instance paths and download execution.
type Service struct {
	Modrinth ModrinthAPI
}

// NewService returns a mod orchestration service. Modrinth must be non-nil.
func NewService(m ModrinthAPI) *Service {
	return &Service{Modrinth: m}
}

// ModsDir is the standard Fabric/Vanilla mods folder for an instance.
func ModsDir(inst *core.Instance) string {
	if inst == nil {
		return ""
	}
	return filepath.Join(inst.Path, ".minecraft", "mods")
}

// SearchFabricMods queries Modrinth for mods compatible with the instance's Minecraft version and Fabric.
func (s *Service) SearchFabricMods(ctx context.Context, inst *core.Instance, query string, offset int) (*api.SearchResult, error) {
	if s == nil || s.Modrinth == nil {
		return nil, fmt.Errorf("modrinth client required")
	}
	if inst == nil {
		return nil, fmt.Errorf("instance required")
	}
	if inst.Version == "" {
		return nil, fmt.Errorf("instance has no Minecraft version")
	}
	q := strings.TrimSpace(query)
	return s.Modrinth.Search(ctx, api.SearchOptions{
		Query:       q,
		Index:       SearchIndex(query),
		Offset:      offset,
		Limit:       20,
		Loaders:     []string{"fabric"},
		GameVersion: inst.Version,
		ProjectType: "mod",
	})
}

// InstallFabricMod resolves the best matching Fabric file for projectID and downloads it into inst.mods.
func (s *Service) InstallFabricMod(ctx context.Context, inst *core.Instance, projectID string) (string, error) {
	if s == nil || s.Modrinth == nil {
		return "", fmt.Errorf("modrinth client required")
	}
	if inst == nil {
		return "", fmt.Errorf("instance required")
	}
	if inst.Version == "" {
		return "", fmt.Errorf("instance has no Minecraft version")
	}
	versions, err := s.Modrinth.GetProjectVersions(ctx, projectID, []string{"fabric"}, []string{inst.Version})
	if err != nil {
		return "", err
	}
	pv := PickBestFabricVersion(versions)
	if pv == nil {
		return "", fmt.Errorf("no Fabric version for Minecraft %s", inst.Version)
	}
	file := PrimaryJar(pv)
	if file == nil || file.URL == "" {
		return "", fmt.Errorf("no downloadable .jar for %s", pv.VersionNumber)
	}
	dir := ModsDir(inst)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return "", fmt.Errorf("mods dir: %w", err)
	}
	dest := filepath.Join(dir, file.Filename)
	mgr := download.NewManager(2)
	result, err := mgr.Download(ctx, []download.Item{{
		URL:  file.URL,
		Path: dest,
		SHA1: file.Hashes.SHA1,
		Size: file.Size,
	}}, nil)
	if err != nil {
		return "", err
	}
	if result.Failed > 0 {
		return "", fmt.Errorf("download failed")
	}
	return dest, nil
}

// InstallFabricModWithDeps resolves the root project plus its transitive required
// dependencies, downloads all jars in one batch, and records each that lands on disk
// in the catalog. Success/failure is attributed by checking file existence (os.Stat)
// rather than parsing per-item download errors.
func (s *Service) InstallFabricModWithDeps(ctx context.Context, inst *core.Instance, projectID string) (*InstallReport, error) {
	if s == nil || s.Modrinth == nil {
		return nil, fmt.Errorf("modrinth client required")
	}
	if inst == nil {
		return nil, fmt.Errorf("instance required")
	}
	if inst.Version == "" {
		return nil, fmt.Errorf("instance has no Minecraft version")
	}
	plan, err := s.ResolveFabricModWithDeps(ctx, inst, projectID)
	if err != nil {
		return nil, err
	}

	dir := ModsDir(inst)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, fmt.Errorf("mods dir: %w", err)
	}

	report := &InstallReport{Skipped: plan.Skipped}

	items := make([]download.Item, 0, len(plan.Mods))
	for _, mod := range plan.Mods {
		items = append(items, download.Item{
			URL:  mod.URL,
			Path: filepath.Join(dir, mod.FileName),
			SHA1: mod.SHA1,
			Size: mod.Size,
		})
	}

	if len(items) > 0 {
		workers := len(items)
		if workers > 4 {
			workers = 4
		}
		mgr := download.NewManager(workers)
		if _, err := mgr.Download(ctx, items, nil); err != nil {
			return nil, err
		}
	}

	// Attribute outcomes by on-disk presence AND content (download manager error
	// mapping is loose). A mod counts as installed only when the file exists and,
	// if we have an expected SHA-1, its content matches — so a stale/colliding
	// pre-existing jar for a different mod isn't miscounted as a success.
	for _, mod := range plan.Mods {
		dest := filepath.Join(dir, mod.FileName)
		if installedOnDisk(dest, mod.SHA1) {
			if err := RecordModrinthInstall(inst, mod.ProjectID, mod.Slug, mod.FileName); err != nil {
				return report, fmt.Errorf("saved %s but catalog: %w", mod.FileName, err)
			}
			report.Installed = append(report.Installed, mod)
		} else {
			report.Failed = append(report.Failed, mod)
		}
	}
	return report, nil
}

// installedOnDisk reports whether dest exists as a regular file and, when
// wantSHA1 is non-empty, its SHA-1 matches (case-insensitive hex).
func installedOnDisk(dest, wantSHA1 string) bool {
	st, err := os.Stat(dest)
	if err != nil || st.IsDir() {
		return false
	}
	if wantSHA1 == "" {
		return true
	}
	got, err := sha1OfFile(dest)
	if err != nil {
		return false
	}
	return strings.EqualFold(got, wantSHA1)
}

// sha1OfFile returns the lowercase hex SHA-1 of the file at path.
func sha1OfFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha1.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}
