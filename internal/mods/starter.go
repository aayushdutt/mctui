package mods

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/aayushdutt/mctui/internal/core"
)

// starterFabricMod is one Modrinth project (slug works with the v2 project API).
// Order matters: Fabric API must load before dependent mods.
type starterFabricMod struct {
	slug  string
	label string
}

// StarterFabricModsDefault is the default “basic Fabric” set for new instances.
// Sodium Extra is omitted: it adds Sodium-related options (fog, particles, etc.) with
// negligible extra performance cost vs Sodium; users can add it from Modrinth if wanted.
var starterFabricModsDefault = []starterFabricMod{
	{slug: "fabric-api", label: "Fabric API"},
	{slug: "modmenu", label: "Mod Menu"},
	{slug: "sodium", label: "Sodium"},
	{slug: "lithium", label: "Lithium"},
}

// StarterFabricModsComplete reports whether every starter mod is recorded in the catalog and its jar still exists.
func StarterFabricModsComplete(inst *core.Instance) bool {
	if inst == nil || inst.Path == "" {
		return false
	}
	c, err := LoadModrinthCatalog(inst)
	if err != nil || c == nil {
		return false
	}
	for _, sm := range starterFabricModsDefault {
		ok := false
		for _, e := range c.Projects {
			if !strings.EqualFold(e.Slug, sm.slug) {
				continue
			}
			p := filepath.Join(ModsDir(inst), filepath.Base(e.File))
			if st, err := os.Stat(p); err == nil && !st.IsDir() {
				ok = true
			}
			break
		}
		if !ok {
			return false
		}
	}
	return true
}

// StarterInstallProgress is called before each mod download (index is 0-based).
type StarterInstallProgress func(index, total int, label string)

// InstallStarterFabricMods installs the default Fabric starter set (Fabric API, Mod Menu, Sodium, Lithium).
// If progress is non-nil, it is invoked before each mod with the human-readable label.
func (s *Service) InstallStarterFabricMods(ctx context.Context, inst *core.Instance, progress StarterInstallProgress) error {
	if s == nil {
		return fmt.Errorf("mods service required")
	}
	if inst == nil || inst.Path == "" {
		return fmt.Errorf("instance with path required")
	}
	total := len(starterFabricModsDefault)
	for i, sm := range starterFabricModsDefault {
		if progress != nil {
			progress(i, total, sm.label)
		}
		path, err := s.InstallFabricMod(ctx, inst, sm.slug)
		if err != nil {
			return fmt.Errorf("%s: %w", sm.label, err)
		}
		if err := RecordModrinthInstall(inst, sm.slug, sm.slug, filepath.Base(path)); err != nil {
			return fmt.Errorf("%s (catalog): %w", sm.label, err)
		}
	}
	return nil
}
