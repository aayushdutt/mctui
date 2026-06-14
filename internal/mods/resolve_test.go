package mods

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

// fakeModrinth implements ModrinthAPI from in-memory fixtures (no network).
type fakeModrinth struct {
	// versionsByProject maps projectID -> versions returned by GetProjectVersions.
	versionsByProject map[string][]api.ProjectVersion
	// versionByID maps version_id -> version returned by GetVersion.
	versionByID map[string]api.ProjectVersion
	// projects maps projectID -> project metadata for GetProject.
	projects map[string]api.Project
}

func (f *fakeModrinth) Search(context.Context, api.SearchOptions) (*api.SearchResult, error) {
	return &api.SearchResult{}, nil
}

func (f *fakeModrinth) GetProject(_ context.Context, idOrSlug string) (*api.Project, error) {
	if p, ok := f.projects[idOrSlug]; ok {
		return &p, nil
	}
	return nil, fmt.Errorf("project not found: %s", idOrSlug)
}

func (f *fakeModrinth) GetProjectVersions(_ context.Context, projectID string, _ []string, _ []string) ([]api.ProjectVersion, error) {
	return f.versionsByProject[projectID], nil
}

func (f *fakeModrinth) GetVersion(_ context.Context, versionID string) (*api.ProjectVersion, error) {
	if v, ok := f.versionByID[versionID]; ok {
		return &v, nil
	}
	return nil, fmt.Errorf("version not found: %s", versionID)
}

const testMC = "1.21.4"

// jarVersion builds a compatible Fabric release version with a primary jar and the given deps.
func jarVersion(projectID, versionID, file string, deps ...api.Dependency) api.ProjectVersion {
	return api.ProjectVersion{
		ID:            versionID,
		ProjectID:     projectID,
		VersionNumber: versionID,
		VersionType:   "release",
		GameVersions:  []string{testMC},
		Loaders:       []string{"fabric"},
		Dependencies:  deps,
		Files: []api.VersionFile{{
			URL:      "https://example.test/" + file,
			Filename: file,
			Primary:  true,
			Size:     100,
			Hashes:   api.FileHashes{SHA1: "sha-" + versionID},
		}},
	}
}

func req(projectID string) api.Dependency {
	return api.Dependency{ProjectID: projectID, DependencyType: "required"}
}

func reqVersion(versionID string) api.Dependency {
	return api.Dependency{VersionID: versionID, DependencyType: "required"}
}

// testInstance returns an instance rooted at a fresh temp dir (empty catalog/jars).
func testInstance(t *testing.T) *core.Instance {
	t.Helper()
	return &core.Instance{Version: testMC, Loader: "fabric", Path: t.TempDir()}
}

func projectIDs(mods []ResolvedMod) []string {
	out := make([]string, len(mods))
	for i, m := range mods {
		out[i] = m.ProjectID
	}
	return out
}

func skipReason(skipped []SkippedDep, projectID string) (string, bool) {
	for _, s := range skipped {
		if s.ProjectID == projectID {
			return s.Reason, true
		}
	}
	return "", false
}

func TestResolveFabricModWithDeps(t *testing.T) {
	t.Run("root plus one required dep, root first", func(t *testing.T) {
		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {jarVersion("root", "root-v1", "root.jar", req("dep"))},
				"dep":  {jarVersion("dep", "dep-v1", "dep.jar")},
			},
		}
		svc := NewService(f)
		plan, err := svc.ResolveFabricModWithDeps(context.Background(), testInstance(t), "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		ids := projectIDs(plan.Mods)
		if len(ids) != 2 || ids[0] != "root" || ids[1] != "dep" {
			t.Fatalf("want [root dep], got %v", ids)
		}
		if !plan.Mods[0].Root {
			t.Fatalf("first mod should be root")
		}
		if plan.Mods[1].Root {
			t.Fatalf("dep should not be root")
		}
		if plan.Mods[1].DepType != "required" {
			t.Fatalf("dep DepType = %q, want required", plan.Mods[1].DepType)
		}
	})

	t.Run("cycle terminates", func(t *testing.T) {
		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"A": {jarVersion("A", "a-v1", "a.jar", req("B"))},
				"B": {jarVersion("B", "b-v1", "b.jar", req("A"))},
			},
		}
		svc := NewService(f)
		plan, err := svc.ResolveFabricModWithDeps(context.Background(), testInstance(t), "A")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		ids := projectIDs(plan.Mods)
		if len(ids) != 2 {
			t.Fatalf("cycle should yield exactly 2 mods, got %v", ids)
		}
	})

	t.Run("diamond resolves shared dep once", func(t *testing.T) {
		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"A": {jarVersion("A", "a-v1", "a.jar", req("B"), req("C"))},
				"B": {jarVersion("B", "b-v1", "b.jar", req("C"))},
				"C": {jarVersion("C", "c-v1", "c.jar")},
			},
		}
		svc := NewService(f)
		plan, err := svc.ResolveFabricModWithDeps(context.Background(), testInstance(t), "A")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		count := 0
		for _, id := range projectIDs(plan.Mods) {
			if id == "C" {
				count++
			}
		}
		if count != 1 {
			t.Fatalf("shared dep C should appear once, got %d (mods=%v)", count, projectIDs(plan.Mods))
		}
		if len(plan.Mods) != 3 {
			t.Fatalf("want 3 mods, got %v", projectIDs(plan.Mods))
		}
	})

	t.Run("optional embedded incompatible are skipped not installed", func(t *testing.T) {
		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {jarVersion("root", "root-v1", "root.jar",
					api.Dependency{ProjectID: "opt", DependencyType: "optional"},
					api.Dependency{ProjectID: "emb", DependencyType: "embedded"},
					api.Dependency{ProjectID: "inc", DependencyType: "incompatible"},
				)},
			},
		}
		svc := NewService(f)
		plan, err := svc.ResolveFabricModWithDeps(context.Background(), testInstance(t), "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(plan.Mods) != 1 {
			t.Fatalf("only root should be installed, got %v", projectIDs(plan.Mods))
		}
		for id, want := range map[string]string{"opt": "optional", "emb": "embedded", "inc": "incompatible"} {
			got, ok := skipReason(plan.Skipped, id)
			if !ok || got != want {
				t.Fatalf("%s: want skip %q, got %q (ok=%v)", id, want, got, ok)
			}
		}
	})

	t.Run("already installed dep is skipped", func(t *testing.T) {
		inst := testInstance(t)
		// Record dep in catalog and place its jar so installedProjectSet finds it.
		if err := EnsureModsDir(inst); err != nil {
			t.Fatal(err)
		}
		writeJar(t, inst, "dep.jar")
		if err := RecordModrinthInstall(inst, "dep", "dep", "dep.jar"); err != nil {
			t.Fatal(err)
		}
		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {jarVersion("root", "root-v1", "root.jar", req("dep"))},
				"dep":  {jarVersion("dep", "dep-v1", "dep.jar")},
			},
		}
		svc := NewService(f)
		plan, err := svc.ResolveFabricModWithDeps(context.Background(), inst, "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if ids := projectIDs(plan.Mods); len(ids) != 1 || ids[0] != "root" {
			t.Fatalf("dep should be skipped, mods=%v", ids)
		}
		if reason, ok := skipReason(plan.Skipped, "dep"); !ok || reason != "already-installed" {
			t.Fatalf("want dep already-installed skip, got %q ok=%v", reason, ok)
		}
	})

	t.Run("dep with no compatible version soft-skips, root remains", func(t *testing.T) {
		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {jarVersion("root", "root-v1", "root.jar", req("dep"))},
				"dep":  {}, // no versions for this MC version
			},
		}
		svc := NewService(f)
		plan, err := svc.ResolveFabricModWithDeps(context.Background(), testInstance(t), "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if ids := projectIDs(plan.Mods); len(ids) != 1 || ids[0] != "root" {
			t.Fatalf("want only root, got %v", ids)
		}
		if reason, ok := skipReason(plan.Skipped, "dep"); !ok || reason != "unresolved" {
			t.Fatalf("want dep unresolved skip, got %q ok=%v", reason, ok)
		}
	})

	t.Run("pinned version_id with mismatched MC version is skipped", func(t *testing.T) {
		mismatch := jarVersion("dep", "dep-pinned", "dep.jar")
		mismatch.GameVersions = []string{"1.20.1"} // not the instance version
		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {jarVersion("root", "root-v1", "root.jar", reqVersion("dep-pinned"))},
			},
			versionByID: map[string]api.ProjectVersion{
				"dep-pinned": mismatch,
			},
		}
		svc := NewService(f)
		plan, err := svc.ResolveFabricModWithDeps(context.Background(), testInstance(t), "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if ids := projectIDs(plan.Mods); len(ids) != 1 || ids[0] != "root" {
			t.Fatalf("want only root, got %v", ids)
		}
		// Pinned deps carry empty ProjectID; verify an "unresolved" skip is present.
		if reason, ok := skipReason(plan.Skipped, ""); !ok || reason != "unresolved" {
			t.Fatalf("want unresolved skip for pinned mismatch, got %q ok=%v", reason, ok)
		}
	})

	t.Run("root with no compatible version errors", func(t *testing.T) {
		f := &fakeModrinth{versionsByProject: map[string][]api.ProjectVersion{"root": {}}}
		svc := NewService(f)
		if _, err := svc.ResolveFabricModWithDeps(context.Background(), testInstance(t), "root"); err == nil {
			t.Fatal("expected error for unresolvable root")
		}
	})

	t.Run("fills slug and title via GetProject", func(t *testing.T) {
		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {jarVersion("root", "root-v1", "root.jar")},
			},
			projects: map[string]api.Project{
				"root": {ID: "root", Slug: "sodium", Title: "Sodium"},
			},
		}
		svc := NewService(f)
		plan, err := svc.ResolveFabricModWithDeps(context.Background(), testInstance(t), "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if plan.Mods[0].Slug != "sodium" || plan.Mods[0].Title != "Sodium" {
			t.Fatalf("meta not filled: %+v", plan.Mods[0])
		}
	})
}

// writeJar creates an empty jar file in the instance mods dir.
func writeJar(t *testing.T, inst *core.Instance, name string) {
	t.Helper()
	if err := EnsureModsDir(inst); err != nil {
		t.Fatal(err)
	}
	p := filepath.Join(ModsDir(inst), name)
	if err := os.WriteFile(p, []byte("jar"), 0644); err != nil {
		t.Fatal(err)
	}
}
