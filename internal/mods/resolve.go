package mods

import (
	"context"
	"fmt"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

// ResolvedMod is one jar selected for installation by the dependency resolver.
type ResolvedMod struct {
	ProjectID string
	Slug      string
	Title     string
	VersionID string
	FileName  string
	URL       string
	SHA1      string
	Size      int64
	Root      bool   // true only for the user-selected project
	DepType   string // "" for root, otherwise "required"
}

// SkippedDep records a dependency that was intentionally not installed.
// Reason is one of: optional, embedded, incompatible, already-installed, unresolved.
type SkippedDep struct {
	ProjectID string
	Reason    string
}

// ResolvePlan is the output of dependency resolution: jars to fetch plus skips.
type ResolvePlan struct {
	Mods    []ResolvedMod
	Skipped []SkippedDep
}

// InstallReport summarizes an install attempt after downloads complete.
type InstallReport struct {
	Installed []ResolvedMod
	Skipped   []SkippedDep
	Failed    []ResolvedMod
}

// resolveNode is an entry in the BFS frontier.
type resolveNode struct {
	projectID string
	versionID string // pinned version, if the dependency specified one
	root      bool
	depType   string
}

// ResolveFabricModWithDeps performs a breadth-first walk from projectID, collecting
// the root jar and every transitively-required dependency jar compatible with the
// instance's Minecraft version + Fabric. Optional, embedded, and incompatible
// dependencies are recorded as skips, never installed. Already-installed projects
// (catalog + on-disk) and unresolvable dependencies are soft-skipped. Cycles and
// diamonds are handled via a visited set keyed by projectID.
func (s *Service) ResolveFabricModWithDeps(ctx context.Context, inst *core.Instance, projectID string) (*ResolvePlan, error) {
	if s == nil || s.Modrinth == nil {
		return nil, fmt.Errorf("modrinth client required")
	}
	if inst == nil {
		return nil, fmt.Errorf("instance required")
	}
	if inst.Version == "" {
		return nil, fmt.Errorf("instance has no Minecraft version")
	}

	installed := s.installedProjectSet(inst)

	plan := &ResolvePlan{}
	visited := map[string]bool{}
	queue := []resolveNode{{projectID: projectID, root: true}}

	for len(queue) > 0 {
		node := queue[0]
		queue = queue[1:]

		// Dedup + cycle guard. Prefer projectID; pinned-only deps key on version_id.
		key := node.projectID
		if key == "" {
			key = "v:" + node.versionID
		}
		if key == "v:" {
			continue
		}
		if visited[key] {
			continue
		}
		visited[key] = true

		// Don't reinstall something already present (root is always installed/refreshed).
		if !node.root && node.projectID != "" && installed[node.projectID] {
			plan.Skipped = append(plan.Skipped, SkippedDep{ProjectID: node.projectID, Reason: "already-installed"})
			continue
		}

		pv, err := s.resolveVersion(ctx, inst, node)
		if err != nil {
			if node.root {
				return nil, err
			}
			plan.Skipped = append(plan.Skipped, SkippedDep{ProjectID: node.projectID, Reason: "unresolved"})
			continue
		}
		if pv == nil {
			if node.root {
				return nil, fmt.Errorf("no Fabric version for Minecraft %s", inst.Version)
			}
			plan.Skipped = append(plan.Skipped, SkippedDep{ProjectID: node.projectID, Reason: "unresolved"})
			continue
		}

		file := PrimaryJar(pv)
		if file == nil || file.URL == "" {
			if node.root {
				return nil, fmt.Errorf("no downloadable .jar for %s", pv.VersionNumber)
			}
			plan.Skipped = append(plan.Skipped, SkippedDep{ProjectID: node.projectID, Reason: "unresolved"})
			continue
		}

		// A pinned version_id-only dep can arrive with an empty ProjectID; backfill
		// it from the resolved version so catalog recording, dedup, and the
		// installed-set key are correct.
		projectID := node.projectID
		if projectID == "" {
			projectID = pv.ProjectID
		}

		mod := ResolvedMod{
			ProjectID: projectID,
			VersionID: pv.ID,
			FileName:  file.Filename,
			URL:       file.URL,
			SHA1:      file.Hashes.SHA1,
			Size:      file.Size,
			Root:      node.root,
			DepType:   node.depType,
		}
		s.fillProjectMeta(ctx, &mod)
		plan.Mods = append(plan.Mods, mod)

		// Enqueue this version's dependencies by type.
		for _, dep := range pv.Dependencies {
			switch dep.DependencyType {
			case "required":
				if dep.ProjectID == "" && dep.VersionID == "" {
					continue
				}
				queue = append(queue, resolveNode{
					projectID: dep.ProjectID,
					versionID: dep.VersionID,
					depType:   "required",
				})
			case "optional":
				plan.Skipped = append(plan.Skipped, SkippedDep{ProjectID: dep.ProjectID, Reason: "optional"})
			case "embedded":
				plan.Skipped = append(plan.Skipped, SkippedDep{ProjectID: dep.ProjectID, Reason: "embedded"})
			case "incompatible":
				plan.Skipped = append(plan.Skipped, SkippedDep{ProjectID: dep.ProjectID, Reason: "incompatible"})
			}
		}
	}

	return plan, nil
}

// resolveVersion picks a concrete, compatible ProjectVersion for a node.
// A pinned version_id is fetched directly and validated against the instance;
// otherwise the project's Fabric versions for the instance MC version are queried.
// Returns (nil, nil) when no compatible version exists.
func (s *Service) resolveVersion(ctx context.Context, inst *core.Instance, node resolveNode) (*api.ProjectVersion, error) {
	if node.versionID != "" {
		pv, err := s.Modrinth.GetVersion(ctx, node.versionID)
		if err != nil {
			return nil, err
		}
		if pv == nil || !versionCompatible(pv, inst.Version) {
			return nil, nil
		}
		return pv, nil
	}

	if node.projectID == "" {
		return nil, nil
	}
	versions, err := s.Modrinth.GetProjectVersions(ctx, node.projectID, []string{"fabric"}, []string{inst.Version})
	if err != nil {
		return nil, err
	}
	pv := PickBestFabricVersion(versions)
	if pv == nil || !versionCompatible(pv, inst.Version) {
		return nil, nil
	}
	return pv, nil
}

// versionCompatible verifies the version targets the instance MC version and Fabric.
func versionCompatible(pv *api.ProjectVersion, mcVersion string) bool {
	if pv == nil {
		return false
	}
	hasGame := false
	for _, gv := range pv.GameVersions {
		if gv == mcVersion {
			hasGame = true
			break
		}
	}
	if !hasGame {
		return false
	}
	for _, l := range pv.Loaders {
		if l == "fabric" {
			return true
		}
	}
	return false
}

// fillProjectMeta best-effort populates Slug/Title via GetProject; failures are ignored
// and the projectID is used as a fallback slug/title.
func (s *Service) fillProjectMeta(ctx context.Context, mod *ResolvedMod) {
	mod.Slug = mod.ProjectID
	mod.Title = mod.ProjectID
	p, err := s.Modrinth.GetProject(ctx, mod.ProjectID)
	if err != nil || p == nil {
		return
	}
	if p.Slug != "" {
		mod.Slug = p.Slug
	}
	if p.Title != "" {
		mod.Title = p.Title
	}
}

// installedProjectSet returns the set of project IDs already present (catalog + on-disk jars).
func (s *Service) installedProjectSet(inst *core.Instance) map[string]bool {
	out := map[string]bool{}
	cat, err := LoadModrinthCatalog(inst)
	if err != nil || cat == nil {
		return out
	}
	jars, _ := ListInstalledJars(inst)
	for _, e := range cat.Projects {
		if IsModrinthProjectInstalled(cat, jars, e.ProjectID, e.Slug) {
			out[e.ProjectID] = true
		}
	}
	return out
}
