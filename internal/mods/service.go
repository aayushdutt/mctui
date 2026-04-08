// Package mods orchestrates Modrinth discovery and installs into instance folders.
// UI and HTTP types stay in internal/api; this layer encodes launcher-specific rules (Fabric + game version).
package mods

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/download"
)

// Service wires Modrinth API calls to instance paths and download execution.
type Service struct {
	Modrinth *api.ModrinthClient
}

// NewService returns a mod orchestration service. Modrinth must be non-nil.
func NewService(m *api.ModrinthClient) *Service {
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
