package mods

import (
	"context"
	"crypto/sha1"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

// sha1hex returns the lowercase hex SHA-1 of body (matches installedOnDisk's hashing).
func sha1hex(body []byte) string {
	sum := sha1.Sum(body)
	return hex.EncodeToString(sum[:])
}

// serveJar registers a handler at /<name> returning body and reports its URL + real SHA-1.
func serveJar(t *testing.T, mux *http.ServeMux, base, name string, body []byte) (url, sha1Hex string) {
	t.Helper()
	mux.HandleFunc("/"+name, func(w http.ResponseWriter, r *http.Request) {
		w.Write(body)
	})
	return base + "/" + name, sha1hex(body)
}

// servedVersion builds a release version whose primary jar points at url with the given SHA-1.
func servedVersion(projectID, versionID, file, url, sha1Hex string, size int64, deps ...api.Dependency) api.ProjectVersion {
	return api.ProjectVersion{
		ID:            versionID,
		ProjectID:     projectID,
		VersionNumber: versionID,
		VersionType:   "release",
		GameVersions:  []string{testMC},
		Loaders:       []string{"fabric"},
		Dependencies:  deps,
		Files: []api.VersionFile{{
			URL:      url,
			Filename: file,
			Primary:  true,
			Size:     size,
			Hashes:   api.FileHashes{SHA1: sha1Hex},
		}},
	}
}

func installedHasProject(report *InstallReport, projectID string) bool {
	for _, m := range report.Installed {
		if m.ProjectID == projectID {
			return true
		}
	}
	return false
}

func failedHasProject(report *InstallReport, projectID string) bool {
	for _, m := range report.Failed {
		if m.ProjectID == projectID {
			return true
		}
	}
	return false
}

func catalogHasProject(t *testing.T, inst *core.Instance, projectID string) bool {
	t.Helper()
	c, err := LoadModrinthCatalog(inst)
	if err != nil {
		t.Fatalf("load catalog: %v", err)
	}
	for _, e := range c.Projects {
		if e.ProjectID == projectID {
			return true
		}
	}
	return false
}

func TestInstallFabricModWithDeps_attribution(t *testing.T) {
	rootBody := []byte("root jar contents")
	depBody := []byte("dependency jar contents")

	t.Run("root and required dep both served install and record", func(t *testing.T) {
		mux := http.NewServeMux()
		srv := httptest.NewServer(mux)
		defer srv.Close()

		rootURL, rootSHA := serveJar(t, mux, srv.URL, "root.jar", rootBody)
		depURL, depSHA := serveJar(t, mux, srv.URL, "dep.jar", depBody)

		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {servedVersion("root", "root-v1", "root.jar", rootURL, rootSHA, int64(len(rootBody)), req("dep"))},
				"dep":  {servedVersion("dep", "dep-v1", "dep.jar", depURL, depSHA, int64(len(depBody)))},
			},
		}
		inst := testInstance(t)
		report, err := NewService(f).InstallFabricModWithDeps(context.Background(), inst, "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.Installed) != 2 || len(report.Failed) != 0 {
			t.Fatalf("want 2 installed / 0 failed, got installed=%v failed=%v", projectIDs(report.Installed), projectIDs(report.Failed))
		}
		for _, name := range []string{"root.jar", "dep.jar"} {
			if _, err := os.Stat(filepath.Join(ModsDir(inst), name)); err != nil {
				t.Fatalf("%s should exist on disk: %v", name, err)
			}
		}
		if !catalogHasProject(t, inst, "root") || !catalogHasProject(t, inst, "dep") {
			t.Fatalf("both root and dep should be recorded in catalog")
		}
	})

	t.Run("dep served wrong bytes fails attribution and is not recorded", func(t *testing.T) {
		mux := http.NewServeMux()
		srv := httptest.NewServer(mux)
		defer srv.Close()

		rootURL, rootSHA := serveJar(t, mux, srv.URL, "root.jar", rootBody)
		// Serve dep with corrupt bytes but advertise the SHA of the clean body.
		depURL, _ := serveJar(t, mux, srv.URL, "dep.jar", []byte("WRONG bytes not matching advertised sha"))
		cleanDepSHA := sha1hex(depBody)

		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {servedVersion("root", "root-v1", "root.jar", rootURL, rootSHA, int64(len(rootBody)), req("dep"))},
				"dep":  {servedVersion("dep", "dep-v1", "dep.jar", depURL, cleanDepSHA, int64(len(depBody)))},
			},
		}
		inst := testInstance(t)
		report, err := NewService(f).InstallFabricModWithDeps(context.Background(), inst, "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if !installedHasProject(report, "root") {
			t.Fatalf("root should install, installed=%v", projectIDs(report.Installed))
		}
		if !failedHasProject(report, "dep") {
			t.Fatalf("dep should be failed (sha mismatch), failed=%v", projectIDs(report.Failed))
		}
		if catalogHasProject(t, inst, "dep") {
			t.Fatalf("dep must NOT be recorded in catalog after sha mismatch")
		}
		if !catalogHasProject(t, inst, "root") {
			t.Fatalf("root should be recorded in catalog")
		}
	})

	t.Run("dep URL 404 fails dep but root installs", func(t *testing.T) {
		mux := http.NewServeMux()
		srv := httptest.NewServer(mux)
		defer srv.Close()

		rootURL, rootSHA := serveJar(t, mux, srv.URL, "root.jar", rootBody)
		// dep.jar is never registered -> ServeMux returns 404.
		depURL := srv.URL + "/dep.jar"

		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {servedVersion("root", "root-v1", "root.jar", rootURL, rootSHA, int64(len(rootBody)), req("dep"))},
				"dep":  {servedVersion("dep", "dep-v1", "dep.jar", depURL, sha1hex(depBody), int64(len(depBody)))},
			},
		}
		inst := testInstance(t)
		report, err := NewService(f).InstallFabricModWithDeps(context.Background(), inst, "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if !installedHasProject(report, "root") {
			t.Fatalf("root should install despite dep 404, installed=%v", projectIDs(report.Installed))
		}
		if !failedHasProject(report, "dep") {
			t.Fatalf("dep should be failed (404), failed=%v", projectIDs(report.Failed))
		}
		if catalogHasProject(t, inst, "dep") {
			t.Fatalf("dep must NOT be recorded after 404")
		}
	})

	t.Run("pre-existing colliding jar with matching expected SHA still fails", func(t *testing.T) {
		mux := http.NewServeMux()
		srv := httptest.NewServer(mux)
		defer srv.Close()

		rootURL, rootSHA := serveJar(t, mux, srv.URL, "root.jar", rootBody)
		// dep download fails (404), but a jar with the dep's *filename* already exists
		// on disk with DIFFERENT content. installedOnDisk must compare the SHA and reject.
		depURL := srv.URL + "/dep.jar"
		expectedDepSHA := sha1hex(depBody)

		inst := testInstance(t)
		if err := EnsureModsDir(inst); err != nil {
			t.Fatal(err)
		}
		collidePath := filepath.Join(ModsDir(inst), "dep.jar")
		if err := os.WriteFile(collidePath, []byte("a totally different mod"), 0644); err != nil {
			t.Fatal(err)
		}

		f := &fakeModrinth{
			versionsByProject: map[string][]api.ProjectVersion{
				"root": {servedVersion("root", "root-v1", "root.jar", rootURL, rootSHA, int64(len(rootBody)), req("dep"))},
				"dep":  {servedVersion("dep", "dep-v1", "dep.jar", depURL, expectedDepSHA, int64(len(depBody)))},
			},
		}
		report, err := NewService(f).InstallFabricModWithDeps(context.Background(), inst, "root")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if installedHasProject(report, "dep") {
			t.Fatalf("colliding stale jar must NOT count as installed dep")
		}
		if !failedHasProject(report, "dep") {
			t.Fatalf("dep should be failed, failed=%v", projectIDs(report.Failed))
		}
		if catalogHasProject(t, inst, "dep") {
			t.Fatalf("dep must NOT be recorded when only a stale colliding jar exists")
		}
	})
}

func TestInstallFabricMod_selectionAndDownload(t *testing.T) {
	releaseBody := []byte("release jar bytes")
	betaBody := []byte("beta jar bytes")

	t.Run("release jar selected and downloaded over beta", func(t *testing.T) {
		mux := http.NewServeMux()
		srv := httptest.NewServer(mux)
		defer srv.Close()

		relURL, relSHA := serveJar(t, mux, srv.URL, "release.jar", releaseBody)
		betaURL, betaSHA := serveJar(t, mux, srv.URL, "beta.jar", betaBody)

		beta := servedVersion("p", "p-beta", "beta.jar", betaURL, betaSHA, int64(len(betaBody)))
		beta.VersionType = "beta"
		release := servedVersion("p", "p-release", "release.jar", relURL, relSHA, int64(len(releaseBody)))

		// beta first to prove PickBestFabricVersion prefers the release.
		f := &fakeModrinth{versionsByProject: map[string][]api.ProjectVersion{"p": {beta, release}}}
		inst := testInstance(t)
		dest, err := NewService(f).InstallFabricMod(context.Background(), inst, "p")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if filepath.Base(dest) != "release.jar" {
			t.Fatalf("want release.jar, got %s", dest)
		}
		got, err := os.ReadFile(dest)
		if err != nil {
			t.Fatalf("read dest: %v", err)
		}
		if string(got) != string(releaseBody) {
			t.Fatalf("downloaded content mismatch: got %q", got)
		}
		if _, err := os.Stat(filepath.Join(ModsDir(inst), "beta.jar")); !os.IsNotExist(err) {
			t.Fatalf("beta.jar should not have been downloaded (err=%v)", err)
		}
	})

	t.Run("version with no jar file errors", func(t *testing.T) {
		noJar := api.ProjectVersion{
			ID:           "v1",
			ProjectID:    "p",
			VersionType:  "release",
			GameVersions: []string{testMC},
			Loaders:      []string{"fabric"},
			Files: []api.VersionFile{{
				URL:      "https://example.test/readme.txt",
				Filename: "readme.txt",
				Primary:  true,
			}},
		}
		f := &fakeModrinth{versionsByProject: map[string][]api.ProjectVersion{"p": {noJar}}}
		_, err := NewService(f).InstallFabricMod(context.Background(), testInstance(t), "p")
		if err == nil || !strings.Contains(err.Error(), "no downloadable .jar") {
			t.Fatalf("want 'no downloadable .jar' error, got %v", err)
		}
	})

	t.Run("empty instance version errors", func(t *testing.T) {
		inst := testInstance(t)
		inst.Version = ""
		_, err := NewService(&fakeModrinth{}).InstallFabricMod(context.Background(), inst, "p")
		if err == nil || !strings.Contains(err.Error(), "no Minecraft version") {
			t.Fatalf("want 'no Minecraft version' error, got %v", err)
		}
	})

	t.Run("no compatible version errors", func(t *testing.T) {
		f := &fakeModrinth{versionsByProject: map[string][]api.ProjectVersion{"p": {}}}
		_, err := NewService(f).InstallFabricMod(context.Background(), testInstance(t), "p")
		if err == nil || !strings.Contains(err.Error(), "no Fabric version") {
			t.Fatalf("want 'no Fabric version' error, got %v", err)
		}
	})
}
