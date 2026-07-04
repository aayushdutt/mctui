package api

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
)

// readGolden loads a fixture from testdata/. Go auto-ignores testdata/ in builds.
func readGolden(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("testdata", name))
	if err != nil {
		t.Fatalf("read fixture %s: %v", name, err)
	}
	return data
}

// The golden tests decode REAL captured upstream responses into the production
// structs and assert the load-bearing required fields are non-zero. An upstream
// rename/removal of any asserted field flips it to its zero value and fails here.

func TestGolden_ModrinthSearch(t *testing.T) {
	t.Parallel()
	var res SearchResult
	if err := json.Unmarshal(readGolden(t, "modrinth_search.json"), &res); err != nil {
		t.Fatalf("decode SearchResult: %v", err)
	}
	if len(res.Hits) == 0 {
		t.Fatal("Hits empty")
	}
	h := res.Hits[0]
	if h.ProjectID == "" {
		t.Error("Hits[0].ProjectID empty")
	}
	if h.Slug == "" {
		t.Error("Hits[0].Slug empty")
	}
	if h.Title == "" {
		t.Error("Hits[0].Title empty")
	}
}

func TestGolden_ModrinthProject(t *testing.T) {
	t.Parallel()
	var p Project
	if err := json.Unmarshal(readGolden(t, "modrinth_project.json"), &p); err != nil {
		t.Fatalf("decode Project: %v", err)
	}
	if p.ID == "" {
		t.Error("ID empty")
	}
	if p.Slug == "" {
		t.Error("Slug empty")
	}
	if p.ProjectType == "" {
		t.Error("ProjectType empty")
	}
	if p.Title == "" {
		t.Error("Title empty")
	}
}

func TestGolden_ModrinthProjectVersions(t *testing.T) {
	t.Parallel()
	var versions []ProjectVersion
	if err := json.Unmarshal(readGolden(t, "modrinth_project_versions.json"), &versions); err != nil {
		t.Fatalf("decode []ProjectVersion: %v", err)
	}
	if len(versions) == 0 {
		t.Fatal("no versions")
	}
	for i, v := range versions {
		if v.ID == "" {
			t.Errorf("versions[%d].ID empty", i)
		}
		if v.VersionNumber == "" {
			t.Errorf("versions[%d].VersionNumber empty", i)
		}
		// Load-bearing download link: every version must carry at least one file
		// with a non-empty URL or the mod cannot be installed.
		if len(v.Files) == 0 {
			t.Errorf("versions[%d].Files empty (no download)", i)
			continue
		}
		if v.Files[0].URL == "" {
			t.Errorf("versions[%d].Files[0].URL empty (load-bearing download link)", i)
		}
	}
}

func TestGolden_ModrinthVersion(t *testing.T) {
	t.Parallel()
	var v ProjectVersion
	if err := json.Unmarshal(readGolden(t, "modrinth_version.json"), &v); err != nil {
		t.Fatalf("decode ProjectVersion: %v", err)
	}
	if v.ID == "" {
		t.Error("ID empty")
	}
	if v.VersionNumber == "" {
		t.Error("VersionNumber empty")
	}
	if len(v.Files) == 0 {
		t.Fatal("Files empty (no download)")
	}
	if v.Files[0].URL == "" {
		t.Error("Files[0].URL empty (load-bearing download link)")
	}
}

func TestGolden_MojangVersionManifest(t *testing.T) {
	t.Parallel()
	var m core.VersionManifest
	if err := json.Unmarshal(readGolden(t, "mojang_version_manifest_v2.json"), &m); err != nil {
		t.Fatalf("decode VersionManifest: %v", err)
	}
	if m.Latest.Release == "" {
		t.Error("Latest.Release empty")
	}
	if len(m.Versions) == 0 {
		t.Fatal("Versions empty")
	}
	for i, v := range m.Versions {
		if v.ID == "" {
			t.Errorf("Versions[%d].ID empty", i)
		}
		if v.Type == "" {
			t.Errorf("Versions[%d].Type empty", i)
		}
		if v.URL == "" {
			t.Errorf("Versions[%d].URL empty", i)
		}
	}
}

func TestGolden_MojangVersionDetail(t *testing.T) {
	t.Parallel()
	var d core.VersionDetails
	if err := json.Unmarshal(readGolden(t, "mojang_version_detail.json"), &d); err != nil {
		t.Fatalf("decode VersionDetails: %v", err)
	}
	if d.MainClass == "" {
		t.Error("MainClass empty")
	}
	if d.AssetIndex.ID == "" {
		t.Error("AssetIndex.ID empty")
	}
	if d.AssetIndex.URL == "" {
		t.Error("AssetIndex.URL empty")
	}
	if d.Downloads.Client == nil || d.Downloads.Client.URL == "" {
		t.Error("Downloads.Client.URL empty")
	}
	if len(d.Libraries) == 0 {
		t.Fatal("Libraries empty")
	}
	if d.Libraries[0].Name == "" {
		t.Error("Libraries[0].Name empty")
	}
	if d.JavaVersion.MajorVersion <= 0 {
		t.Errorf("JavaVersion.MajorVersion = %d, want > 0", d.JavaVersion.MajorVersion)
	}
}

func TestGolden_VanillaTweaksCatalog(t *testing.T) {
	t.Parallel()
	var cat ResourcePackCatalog
	if err := json.Unmarshal(readGolden(t, "vanillatweaks_rpcategories.json"), &cat); err != nil {
		t.Fatalf("decode ResourcePackCatalog: %v", err)
	}
	if len(cat.Categories) == 0 {
		t.Fatal("Categories empty")
	}
	if cat.Categories[0].Category == "" {
		t.Error("Categories[0].Category empty")
	}
}

func TestGolden_VanillaTweaksBuild(t *testing.T) {
	t.Parallel()
	var b vtBuildResponse
	if err := json.Unmarshal(readGolden(t, "vanillatweaks_build.json"), &b); err != nil {
		t.Fatalf("decode vtBuildResponse: %v", err)
	}
	if b.Status != "success" {
		t.Errorf("Status = %q, want success", b.Status)
	}
	if b.Link == "" {
		t.Error("Link empty")
	}
}

// TestGolden_AdoptiumFeatureReleases walks the Adoptium feature_releases response
// with DEFENSIVE comma-ok type assertions at every level. The java client lives
// in another package; this asserts the SHAPE the java client depends on so an
// upstream restructure is caught here. It documents the load-bearing path
// releases[0].binaries[0].package.{link,name}.
func TestGolden_AdoptiumFeatureReleases(t *testing.T) {
	t.Parallel()
	var releases []interface{}
	if err := json.Unmarshal(readGolden(t, "adoptium_feature_releases.json"), &releases); err != nil {
		t.Fatalf("decode adoptium releases: %v", err)
	}
	if len(releases) == 0 {
		t.Fatal("no releases")
	}

	rel, ok := releases[0].(map[string]interface{})
	if !ok {
		t.Fatalf("releases[0] is %T, want object", releases[0])
	}
	binsRaw, ok := rel["binaries"]
	if !ok {
		t.Fatal("releases[0].binaries missing")
	}
	bins, ok := binsRaw.([]interface{})
	if !ok || len(bins) == 0 {
		t.Fatalf("releases[0].binaries is %T (len check), want non-empty array", binsRaw)
	}
	bin, ok := bins[0].(map[string]interface{})
	if !ok {
		t.Fatalf("releases[0].binaries[0] is %T, want object", bins[0])
	}
	pkgRaw, ok := bin["package"]
	if !ok {
		t.Fatal("releases[0].binaries[0].package missing")
	}
	pkg, ok := pkgRaw.(map[string]interface{})
	if !ok {
		t.Fatalf("releases[0].binaries[0].package is %T, want object", pkgRaw)
	}
	link, ok := pkg["link"].(string)
	if !ok || link == "" {
		t.Errorf("package.link = %v (%T), want non-empty string", pkg["link"], pkg["link"])
	}
	name, ok := pkg["name"].(string)
	if !ok || name == "" {
		t.Errorf("package.name = %v (%T), want non-empty string", pkg["name"], pkg["name"])
	}
}
