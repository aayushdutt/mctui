package api

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestModrinthClient_Search_ok(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !strings.HasPrefix(r.URL.Path, "/search") {
			t.Errorf("path = %q, want /search", r.URL.Path)
		}
		if got := r.URL.Query().Get("query"); got != "sodium" {
			t.Errorf("query = %q, want sodium", got)
		}
		if got := r.Header.Get("User-Agent"); got != userAgent {
			t.Errorf("User-Agent = %q, want %q", got, userAgent)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(SearchResult{
			Hits:      []SearchHit{{ProjectID: "AANobbMI", Slug: "sodium", Title: "Sodium"}},
			TotalHits: 1,
			Limit:     20,
		})
	}))
	defer ts.Close()

	c := NewModrinthClientWithBaseURL(ts.URL)
	res, err := c.Search(context.Background(), SearchOptions{Query: "sodium", ProjectType: "mod"})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if res.TotalHits != 1 || len(res.Hits) != 1 {
		t.Fatalf("got %d hits (total %d), want 1", len(res.Hits), res.TotalHits)
	}
	if res.Hits[0].Slug != "sodium" {
		t.Errorf("slug = %q, want sodium", res.Hits[0].Slug)
	}
}

func TestModrinthClient_Search_non200(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer ts.Close()

	c := NewModrinthClientWithBaseURL(ts.URL)
	if _, err := c.Search(context.Background(), SearchOptions{Query: "x"}); err == nil {
		t.Fatal("expected error on 500, got nil")
	}
}

func TestModrinthClient_GetProject(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		status  int
		body    string
		wantErr bool
		wantID  string
	}{
		{name: "ok", status: 200, body: `{"id":"AANobbMI","slug":"sodium","title":"Sodium"}`, wantID: "AANobbMI"},
		{name: "not found", status: 404, body: "", wantErr: true},
		{name: "server error", status: 503, body: "", wantErr: true},
		{name: "malformed json", status: 200, body: `{not json`, wantErr: true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if want := "/project/sodium"; r.URL.Path != want {
					t.Errorf("path = %q, want %q", r.URL.Path, want)
				}
				w.WriteHeader(tc.status)
				_, _ = w.Write([]byte(tc.body))
			}))
			defer ts.Close()

			c := NewModrinthClientWithBaseURL(ts.URL)
			proj, err := c.GetProject(context.Background(), "sodium")
			if tc.wantErr {
				if err == nil {
					t.Fatal("expected error, got nil")
				}
				return
			}
			if err != nil {
				t.Fatalf("GetProject: %v", err)
			}
			if proj.ID != tc.wantID {
				t.Errorf("ID = %q, want %q", proj.ID, tc.wantID)
			}
		})
	}
}

func TestModrinthClient_GetProjectVersions_ok(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if want := "/project/AANobbMI/version"; r.URL.Path != want {
			t.Errorf("path = %q, want %q", r.URL.Path, want)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode([]ProjectVersion{
			{ID: "v1", VersionNumber: "0.5.0", Loaders: []string{"fabric"}},
		})
	}))
	defer ts.Close()

	c := NewModrinthClientWithBaseURL(ts.URL)
	vers, err := c.GetProjectVersions(context.Background(), "AANobbMI", []string{"fabric"}, []string{"1.21"})
	if err != nil {
		t.Fatalf("GetProjectVersions: %v", err)
	}
	if len(vers) != 1 || vers[0].VersionNumber != "0.5.0" {
		t.Fatalf("got %+v, want one version 0.5.0", vers)
	}
}

func TestModrinthClient_GetVersion_ok(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if want := "/version/v1"; r.URL.Path != want {
			t.Errorf("path = %q, want %q", r.URL.Path, want)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(ProjectVersion{
			ID:    "v1",
			Files: []VersionFile{{URL: "https://cdn/sodium.jar", Filename: "sodium.jar", Primary: true}},
		})
	}))
	defer ts.Close()

	c := NewModrinthClientWithBaseURL(ts.URL)
	v, err := c.GetVersion(context.Background(), "v1")
	if err != nil {
		t.Fatalf("GetVersion: %v", err)
	}
	if len(v.Files) != 1 || v.Files[0].Filename != "sodium.jar" {
		t.Fatalf("got %+v, want one file sodium.jar", v.Files)
	}
}

func TestModrinthClient_contextCancelled(t *testing.T) {
	t.Parallel()
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))
	defer ts.Close()

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // cancel before the call

	c := NewModrinthClientWithBaseURL(ts.URL)
	if _, err := c.Search(ctx, SearchOptions{Query: "x"}); err == nil {
		t.Fatal("expected error from cancelled context, got nil")
	}
}
