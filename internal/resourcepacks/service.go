// Package resourcepacks orchestrates Vanilla Tweaks discovery, cart persistence,
// and merged-pack application into instance folders.
//
// UI and HTTP types stay in internal/api; this layer encodes launcher-specific
// rules (version normalization, options.txt wiring, the persistent per-instance
// cart). It mirrors internal/mods: a consumer-side API interface, a Service that
// wires API calls to instance paths and download execution, plus catalog/cart
// persistence under .minecraft/resourcepacks.
package resourcepacks

import (
	"context"
	"fmt"
	"path/filepath"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

// MergedPackFileName is the basename of the merged resource pack written into the
// instance resourcepacks folder. options.txt references it as "file/<this>".
const MergedPackFileName = "VanillaTweaks.zip"

// VanillaTweaksAPI is the subset of *api.VanillaTweaksClient the resourcepacks
// layer depends on. Declared consumer-side so the service can be unit-tested with
// a fake. *api.VanillaTweaksClient satisfies this interface.
type VanillaTweaksAPI interface {
	FetchResourcePackCatalog(ctx context.Context, version string) (*api.ResourcePackCatalog, error)
	BuildResourcePackZip(ctx context.Context, selection map[string][]string, version string) (downloadURL string, err error)
	DownloadResourcePackZip(ctx context.Context, downloadURL, dest string) error
}

// Service wires Vanilla Tweaks API calls to instance paths and download execution.
type Service struct {
	VT VanillaTweaksAPI
}

// NewService returns a resource-pack orchestration service. vt must be non-nil.
func NewService(vt VanillaTweaksAPI) *Service {
	return &Service{VT: vt}
}

// ResourcePacksDir is the standard resource-packs folder for an instance.
func ResourcePacksDir(inst *core.Instance) string {
	if inst == nil {
		return ""
	}
	return filepath.Join(inst.Path, ".minecraft", "resourcepacks")
}

// MergedPackPath is the absolute path of the merged pack zip for an instance.
func MergedPackPath(inst *core.Instance) string {
	if inst == nil {
		return ""
	}
	return filepath.Join(ResourcePacksDir(inst), MergedPackFileName)
}

// CatalogVersionFor returns the Vanilla Tweaks catalog version to use for the
// instance's Minecraft version, plus whether a published catalog is expected.
func CatalogVersionFor(inst *core.Instance) (version string, ok bool) {
	if inst == nil {
		return "", false
	}
	return api.NormalizeVTVersion(inst.Version)
}

// FetchCatalog fetches the resource-pack catalog for the instance's (normalized)
// Minecraft version.
func (s *Service) FetchCatalog(ctx context.Context, inst *core.Instance) (*api.ResourcePackCatalog, error) {
	if s == nil || s.VT == nil {
		return nil, fmt.Errorf("vanilla tweaks client required")
	}
	if inst == nil {
		return nil, fmt.Errorf("instance required")
	}
	version, _ := CatalogVersionFor(inst)
	return s.VT.FetchResourcePackCatalog(ctx, version)
}
