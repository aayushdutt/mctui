package fabric

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// withMetaBase points the Fabric meta resolver at srvURL for the duration of the
// test and restores it afterwards. metaBase is a process-global package var, so
// tests using this helper must not call t.Parallel.
func withMetaBase(t *testing.T, srvURL string) {
	t.Helper()
	old := metaBase
	metaBase = srvURL
	t.Cleanup(func() { metaBase = old })
}

func TestPickStableLoaderVersion(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if want := "/v2/versions/loader/1.21"; r.URL.Path != want {
			t.Errorf("path = %q, want %q", r.URL.Path, want)
		}
		// First entry unstable, second stable: the resolver must skip to the stable one.
		_, _ = w.Write([]byte(`[
			{"loader":{"version":"0.17.0-beta","stable":false}},
			{"loader":{"version":"0.16.10","stable":true}}
		]`))
	}))
	defer ts.Close()
	withMetaBase(t, ts.URL)

	got, err := pickStableLoaderVersion(context.Background(), "1.21")
	if err != nil {
		t.Fatalf("pickStableLoaderVersion: %v", err)
	}
	if got != "0.16.10" {
		t.Errorf("loader version = %q, want 0.16.10 (first stable)", got)
	}
}

func TestPickStableLoaderVersion_errors(t *testing.T) {
	tests := []struct {
		name   string
		status int
		body   string
	}{
		{name: "non-200", status: 500, body: ""},
		{name: "empty list", status: 200, body: `[]`},
		{name: "malformed json", status: 200, body: `{not json`},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(tc.status)
				_, _ = w.Write([]byte(tc.body))
			}))
			defer ts.Close()
			withMetaBase(t, ts.URL)

			if _, err := pickStableLoaderVersion(context.Background(), "1.21"); err == nil {
				t.Fatal("expected error, got nil")
			}
		})
	}
}

func TestFetchBytes_non200IncludesBody(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		_, _ = w.Write([]byte("no such loader"))
	}))
	defer ts.Close()

	_, err := fetchBytes(context.Background(), ts.URL+"/missing")
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if !strings.Contains(err.Error(), "404") {
		t.Errorf("error %q should mention status 404", err)
	}
}
