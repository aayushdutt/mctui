//go:build live

package api

import (
	"context"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
)

// Live tests hit REAL upstream endpoints via the production constructors and
// reuse the same required-field assertions as the golden tests. They are gated
// behind the `live` build tag and excluded from the normal suite.

func liveCtx(t *testing.T) context.Context {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	t.Cleanup(cancel)
	return ctx
}

func TestLive_ModrinthChain(t *testing.T) {
	ctx := liveCtx(t)
	c := NewModrinthClient()

	res, err := c.Search(ctx, SearchOptions{Query: "sodium"})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(res.Hits) == 0 {
		t.Fatal("Hits empty")
	}
	h := res.Hits[0]
	if h.ProjectID == "" || h.Slug == "" || h.Title == "" {
		t.Fatalf("hit missing required fields: %+v", h)
	}

	proj, err := c.GetProject(ctx, h.Slug)
	if err != nil {
		t.Fatalf("GetProject(%s): %v", h.Slug, err)
	}
	if proj.ID == "" || proj.Slug == "" || proj.ProjectType == "" || proj.Title == "" {
		t.Fatalf("project missing required fields: %+v", proj)
	}

	versions, err := c.GetProjectVersions(ctx, proj.Slug, nil, nil)
	if err != nil {
		t.Fatalf("GetProjectVersions: %v", err)
	}
	if len(versions) == 0 {
		t.Fatal("no versions")
	}
	for i, v := range versions {
		if v.ID == "" || v.VersionNumber == "" {
			t.Fatalf("versions[%d] missing id/version_number: %+v", i, v)
		}
		if len(v.Files) == 0 || v.Files[0].URL == "" {
			t.Fatalf("versions[%d] missing load-bearing download link", i)
		}
	}

	v, err := c.GetVersion(ctx, versions[0].ID)
	if err != nil {
		t.Fatalf("GetVersion(%s): %v", versions[0].ID, err)
	}
	if len(v.Files) == 0 || v.Files[0].URL == "" {
		t.Fatal("version Files[0].URL empty (load-bearing download link)")
	}
}

func TestLive_MojangChain(t *testing.T) {
	ctx := liveCtx(t)
	c := NewMojangClient(t.TempDir())

	manifest, err := c.GetVersionManifest(ctx)
	if err != nil {
		t.Fatalf("GetVersionManifest: %v", err)
	}
	if manifest.Latest.Release == "" {
		t.Fatal("Latest.Release empty")
	}
	if len(manifest.Versions) == 0 {
		t.Fatal("Versions empty")
	}

	rel, err := c.GetLatestRelease(ctx)
	if err != nil {
		t.Fatalf("GetLatestRelease: %v", err)
	}
	if rel == "" {
		t.Fatal("latest release id empty")
	}

	version, err := c.FindVersion(ctx, rel)
	if err != nil {
		t.Fatalf("FindVersion(%s): %v", rel, err)
	}
	if version.ID == "" || version.Type == "" || version.URL == "" {
		t.Fatalf("version missing required fields: %+v", version)
	}

	d, err := c.GetVersionDetails(ctx, version)
	if err != nil {
		t.Fatalf("GetVersionDetails: %v", err)
	}
	if d.MainClass == "" {
		t.Error("MainClass empty")
	}
	if d.AssetIndex.ID == "" || d.AssetIndex.URL == "" {
		t.Error("AssetIndex.ID/URL empty")
	}
	if d.Downloads.Client == nil || d.Downloads.Client.URL == "" {
		t.Error("Downloads.Client.URL empty")
	}
	if len(d.Libraries) == 0 || d.Libraries[0].Name == "" {
		t.Error("Libraries empty or [0].Name empty")
	}
	if d.JavaVersion.MajorVersion <= 0 {
		t.Errorf("JavaVersion.MajorVersion = %d, want > 0", d.JavaVersion.MajorVersion)
	}
	_ = core.VersionTypeRelease // keep core import meaningful
}

func TestLive_VanillaTweaksCatalog(t *testing.T) {
	ctx := liveCtx(t)
	c := NewVanillaTweaksClient()

	cat, err := c.FetchResourcePackCatalog(ctx, "1.21")
	if err != nil {
		t.Fatalf("FetchResourcePackCatalog: %v", err)
	}
	if len(cat.Categories) == 0 {
		t.Fatal("Categories empty")
	}
	if cat.Categories[0].Category == "" {
		t.Error("Categories[0].Category empty")
	}
}
