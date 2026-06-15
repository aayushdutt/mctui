package api

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

func TestVanillaTweaks_FetchResourcePackCatalog_ok(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if want := "/assets/resources/json/1.21/rpcategories.json"; r.URL.Path != want {
			t.Errorf("path = %q, want %q", r.URL.Path, want)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(ResourcePackCatalog{
			Categories: []RPCategory{{
				Category: "Aesthetic",
				Packs:    []RPPack{{Name: "BorderlessGlass", Display: "Borderless Glass"}},
			}},
		})
	}))
	defer ts.Close()

	c := NewVanillaTweaksClientWithBaseURL(ts.URL)
	cat, err := c.FetchResourcePackCatalog(context.Background(), "1.21.4")
	if err != nil {
		t.Fatalf("FetchResourcePackCatalog: %v", err)
	}
	if cat.Version != "1.21" {
		t.Errorf("Version = %q, want 1.21 (normalized)", cat.Version)
	}
	if len(cat.Categories) != 1 || len(cat.Categories[0].Packs) != 1 {
		t.Fatalf("got %+v, want one category with one pack", cat.Categories)
	}
}

func TestVanillaTweaks_FetchResourcePackCatalog_notFound(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
	}))
	defer ts.Close()

	c := NewVanillaTweaksClientWithBaseURL(ts.URL)
	if _, err := c.FetchResourcePackCatalog(context.Background(), "1.21"); err == nil {
		t.Fatal("expected error on 404, got nil")
	}
}

func TestVanillaTweaks_BuildResourcePackZip(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name      string
		status    int
		body      string
		selection map[string][]string
		wantErr   bool
		wantURL   string // relative to server root (absolute is built from baseURL)
	}{
		{
			name:      "success returns absolute link",
			status:    200,
			body:      `{"status":"success","link":"/download/pack.zip"}`,
			selection: map[string][]string{"Aesthetic": {"BorderlessGlass"}},
			wantURL:   "/download/pack.zip",
		},
		{
			name:      "build status not success",
			status:    200,
			body:      `{"status":"error","link":""}`,
			selection: map[string][]string{"Aesthetic": {"BorderlessGlass"}},
			wantErr:   true,
		},
		{
			name:      "empty selection rejected before any request",
			selection: map[string][]string{},
			wantErr:   true,
		},
		{
			name:      "non-200",
			status:    500,
			selection: map[string][]string{"Aesthetic": {"BorderlessGlass"}},
			wantErr:   true,
		},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.Method != http.MethodPost {
					t.Errorf("method = %s, want POST", r.Method)
				}
				if got := r.FormValue("version"); got != "1.21" {
					t.Errorf("version form value = %q, want 1.21", got)
				}
				w.WriteHeader(tc.status)
				_, _ = w.Write([]byte(tc.body))
			}))
			defer ts.Close()

			c := NewVanillaTweaksClientWithBaseURL(ts.URL)
			got, err := c.BuildResourcePackZip(context.Background(), tc.selection, "1.21")
			if tc.wantErr {
				if err == nil {
					t.Fatal("expected error, got nil")
				}
				return
			}
			if err != nil {
				t.Fatalf("BuildResourcePackZip: %v", err)
			}
			if want := ts.URL + tc.wantURL; got != want {
				t.Errorf("download URL = %q, want %q", got, want)
			}
		})
	}
}

func TestVanillaTweaks_DownloadResourcePackZip(t *testing.T) {
	t.Parallel()
	const payload = "PK\x03\x04 fake zip bytes"
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(payload))
	}))
	defer ts.Close()

	dest := filepath.Join(t.TempDir(), "nested", "pack.zip")
	c := NewVanillaTweaksClientWithBaseURL(ts.URL)
	if err := c.DownloadResourcePackZip(context.Background(), ts.URL+"/download/pack.zip", dest); err != nil {
		t.Fatalf("DownloadResourcePackZip: %v", err)
	}

	got, err := os.ReadFile(dest)
	if err != nil {
		t.Fatalf("reading downloaded file: %v", err)
	}
	if string(got) != payload {
		t.Errorf("downloaded contents = %q, want %q", got, payload)
	}
	// The temp file should not be left behind after an atomic rename.
	if _, err := os.Stat(dest + ".tmp"); !os.IsNotExist(err) {
		t.Errorf(".tmp file left behind: err=%v", err)
	}
}

func TestVanillaTweaks_DownloadResourcePackZip_non200(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
	}))
	defer ts.Close()

	dest := filepath.Join(t.TempDir(), "pack.zip")
	c := NewVanillaTweaksClientWithBaseURL(ts.URL)
	if err := c.DownloadResourcePackZip(context.Background(), ts.URL+"/x.zip", dest); err == nil {
		t.Fatal("expected error on 404, got nil")
	}
	if _, err := os.Stat(dest); !os.IsNotExist(err) {
		t.Error("dest file should not exist after a failed download")
	}
}
