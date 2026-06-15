package java

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// newTestDownloader points the downloader at srvURL and disables retryablehttp's
// exponential backoff so error-path tests fail fast instead of retrying for
// seconds against the test server.
func newTestDownloader(srvURL string) *Downloader {
	d := NewDownloaderWithBaseURL(srvURL)
	d.client.RetryMax = 0
	return d
}

func TestResolveAdoptiumURL_ok(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !strings.HasPrefix(r.URL.Path, "/v3/assets/feature_releases/21/ga") {
			t.Errorf("path = %q, want adoptium feature_releases path", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`[{"binaries":[{"package":{"link":"https://cdn/jre.tar.gz","name":"jre.tar.gz"}}]}]`))
	}))
	defer ts.Close()

	d := newTestDownloader(ts.URL)
	link, name, err := d.resolveAdoptiumURL(context.Background(), 21)
	if err != nil {
		t.Fatalf("resolveAdoptiumURL: %v", err)
	}
	if link != "https://cdn/jre.tar.gz" {
		t.Errorf("link = %q, want https://cdn/jre.tar.gz", link)
	}
	if name != "jre.tar.gz" {
		t.Errorf("name = %q, want jre.tar.gz", name)
	}
}

func TestResolveAdoptiumURL_errors(t *testing.T) {
	tests := []struct {
		name   string
		status int
		body   string
	}{
		{name: "non-200", status: 500, body: ""},
		{name: "empty releases", status: 200, body: `[]`},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(tc.status)
				_, _ = w.Write([]byte(tc.body))
			}))
			defer ts.Close()

			d := newTestDownloader(ts.URL)
			if _, _, err := d.resolveAdoptiumURL(context.Background(), 21); err == nil {
				t.Fatal("expected error, got nil")
			}
		})
	}
}

// TestDownloadRuntime_endToEnd exercises the full resolve -> download -> extract
// -> find-executable chain against a server that returns a real tar.gz whose
// top-level directory is stripped on extraction.
func TestDownloadRuntime_endToEnd(t *testing.T) {
	archive := makeTarGz(t, map[string]string{
		"jdk-21.0.4/bin/java":     "#!/bin/sh\n",
		"jdk-21.0.4/lib/whatever": "data",
		"jdk-21.0.4/release":      "JAVA_VERSION=21",
	})

	mux := http.NewServeMux()
	srv := httptest.NewServer(mux)
	defer srv.Close()
	// Handlers are registered after the server starts so the resolve response can
	// point at this same server's archive URL (srv.URL). http.ServeMux is safe for
	// registration concurrent with serving, and no request arrives until
	// DownloadRuntime is called below.
	mux.HandleFunc("/archive/jre.tar.gz", func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write(archive)
	})
	mux.HandleFunc("/v3/assets/feature_releases/", func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(fmt.Sprintf(`[{"binaries":[{"package":{"link":%q,"name":"jre.tar.gz"}}]}]`, srv.URL+"/archive/jre.tar.gz")))
	})

	d := newTestDownloader(srv.URL)
	javaPath, err := d.DownloadRuntime(context.Background(), 21, t.TempDir(), func(string) {})
	if err != nil {
		t.Fatalf("DownloadRuntime: %v", err)
	}
	if !strings.HasSuffix(javaPath, "bin/java") {
		t.Errorf("java path = %q, want suffix bin/java", javaPath)
	}
}

func makeTarGz(t *testing.T, files map[string]string) []byte {
	t.Helper()
	var buf bytes.Buffer
	gz := gzip.NewWriter(&buf)
	tw := tar.NewWriter(gz)
	for name, content := range files {
		if err := tw.WriteHeader(&tar.Header{
			Name:     name,
			Mode:     0o755,
			Size:     int64(len(content)),
			Typeflag: tar.TypeReg,
		}); err != nil {
			t.Fatalf("tar header: %v", err)
		}
		if _, err := tw.Write([]byte(content)); err != nil {
			t.Fatalf("tar write: %v", err)
		}
	}
	if err := tw.Close(); err != nil {
		t.Fatalf("tar close: %v", err)
	}
	if err := gz.Close(); err != nil {
		t.Fatalf("gzip close: %v", err)
	}
	return buf.Bytes()
}
