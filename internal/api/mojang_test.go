package api

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
)

// mojangManifestServer serves a version manifest whose single version's detail
// URL points back at the same server (path /version/<id>.json), so a client
// pointed at it exercises both the manifest fetch and the detail fetch.
func mojangManifestServer(t *testing.T) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(nil)
	mux := http.NewServeMux()
	mux.HandleFunc("/manifest.json", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(core.VersionManifest{
			Latest: core.LatestVersions{Release: "1.21", Snapshot: "24w20a"},
			Versions: []core.Version{
				{ID: "1.21", Type: "release", URL: srv.URL + "/version/1.21.json"},
			},
		})
	})
	mux.HandleFunc("/version/1.21.json", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(core.VersionDetails{ID: "1.21", MainClass: "net.minecraft.client.main.Main"})
	})
	srv.Config.Handler = mux
	return srv
}

func TestMojangClient_GetVersionManifest_ok(t *testing.T) {
	t.Parallel()
	srv := mojangManifestServer(t)
	defer srv.Close()

	c := NewMojangClientWithManifestURL(t.TempDir(), srv.URL+"/manifest.json")
	m, err := c.GetVersionManifest(context.Background())
	if err != nil {
		t.Fatalf("GetVersionManifest: %v", err)
	}
	if m.Latest.Release != "1.21" || len(m.Versions) != 1 {
		t.Fatalf("got %+v, want release 1.21 with one version", m.Latest)
	}
}

func TestMojangClient_ManifestCached(t *testing.T) {
	t.Parallel()
	var hits int
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits++
		_ = json.NewEncoder(w).Encode(core.VersionManifest{Latest: core.LatestVersions{Release: "1.21"}})
	}))
	defer srv.Close()

	c := NewMojangClientWithManifestURL(t.TempDir(), srv.URL)
	for i := 0; i < 3; i++ {
		if _, err := c.GetVersionManifest(context.Background()); err != nil {
			t.Fatalf("GetVersionManifest #%d: %v", i, err)
		}
	}
	if hits != 1 {
		t.Errorf("server hit %d times, want 1 (manifest should be cached within TTL)", hits)
	}
}

func TestMojangClient_GetVersionManifest_non200(t *testing.T) {
	t.Parallel()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadGateway)
	}))
	defer srv.Close()

	c := NewMojangClientWithManifestURL(t.TempDir(), srv.URL)
	if _, err := c.GetVersionManifest(context.Background()); err == nil {
		t.Fatal("expected error on 502, got nil")
	}
}

func TestMojangClient_GetVersionManifest_malformed(t *testing.T) {
	t.Parallel()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{not json`))
	}))
	defer srv.Close()

	c := NewMojangClientWithManifestURL(t.TempDir(), srv.URL)
	if _, err := c.GetVersionManifest(context.Background()); err == nil {
		t.Fatal("expected decode error, got nil")
	}
}

func TestMojangClient_FindVersionAndDetails(t *testing.T) {
	t.Parallel()
	srv := mojangManifestServer(t)
	defer srv.Close()

	c := NewMojangClientWithManifestURL(t.TempDir(), srv.URL+"/manifest.json")

	v, err := c.FindVersion(context.Background(), "1.21")
	if err != nil {
		t.Fatalf("FindVersion: %v", err)
	}
	details, err := c.GetVersionDetails(context.Background(), v)
	if err != nil {
		t.Fatalf("GetVersionDetails: %v", err)
	}
	if details.MainClass == "" {
		t.Error("MainClass empty — version detail fetch/decode did not populate it")
	}

	if _, err := c.FindVersion(context.Background(), "nope"); err == nil {
		t.Error("expected error for unknown version, got nil")
	}
}

func TestMojangClient_GetLatestRelease(t *testing.T) {
	t.Parallel()
	srv := mojangManifestServer(t)
	defer srv.Close()

	c := NewMojangClientWithManifestURL(t.TempDir(), srv.URL+"/manifest.json")
	rel, err := c.GetLatestRelease(context.Background())
	if err != nil {
		t.Fatalf("GetLatestRelease: %v", err)
	}
	if rel != "1.21" {
		t.Errorf("latest release = %q, want 1.21", rel)
	}
}
