package java

import (
	"archive/tar"
	"archive/zip"
	"bytes"
	"compress/gzip"
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
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

// makeZip builds an in-memory .zip from files (forward-slash names, as zip
// requires). A name ending in "/" is written as an explicit directory entry.
func makeZip(t *testing.T, files map[string]string) []byte {
	t.Helper()
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	for name, content := range files {
		if strings.HasSuffix(name, "/") {
			// Explicit directory entry: a non-regular header so f.FileInfo().IsDir() is true.
			if _, err := zw.CreateHeader(&zip.FileHeader{Name: name}); err != nil {
				t.Fatalf("zip dir header: %v", err)
			}
			continue
		}
		w, err := zw.Create(name)
		if err != nil {
			t.Fatalf("zip create: %v", err)
		}
		if _, err := w.Write([]byte(content)); err != nil {
			t.Fatalf("zip write: %v", err)
		}
	}
	if err := zw.Close(); err != nil {
		t.Fatalf("zip close: %v", err)
	}
	return buf.Bytes()
}

// TestExtractZip_roundTrip mirrors the tar.gz e2e but for .zip: the resolve
// handler returns a link/name ending in .zip so extractArchive routes to
// extractZip. The archive includes an explicit dir entry, a bare top-level
// entry, and nested files to exercise extractZip's branches.
func TestExtractZip_roundTrip(t *testing.T) {
	archive := makeZip(t, map[string]string{
		"jdk-21/bin/java":     "#!/bin/sh\n",
		"jdk-21/lib/whatever": "data",
		"jdk-21/lib/":         "", // explicit directory entry -> IsDir() branch
		"jdk-21/":             "", // bare top-level only -> len(parts) <= 1 skip
	})

	mux := http.NewServeMux()
	srv := httptest.NewServer(mux)
	defer srv.Close()
	mux.HandleFunc("/archive/jre.zip", func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write(archive)
	})
	mux.HandleFunc("/v3/assets/feature_releases/", func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(fmt.Sprintf(`[{"binaries":[{"package":{"link":%q,"name":"jre.zip"}}]}]`, srv.URL+"/archive/jre.zip")))
	})

	versionDir := t.TempDir()
	d := newTestDownloader(srv.URL)
	javaPath, err := d.DownloadRuntime(context.Background(), 21, versionDir, func(string) {})
	if err != nil {
		t.Fatalf("DownloadRuntime: %v", err)
	}
	if !strings.HasSuffix(filepath.ToSlash(javaPath), "bin/java") {
		t.Errorf("java path = %q, want suffix bin/java", javaPath)
	}

	// Top-level dir stripped: file lands at <versionDir>/21/bin/java, not .../jdk-21/bin/java.
	wantJava := filepath.Join(versionDir, "21", "bin", "java")
	if javaPath != wantJava {
		t.Errorf("java path = %q, want %q (top-level dir stripped)", javaPath, wantJava)
	}
	if _, err := os.Stat(filepath.Join(versionDir, "21", "jdk-21")); !os.IsNotExist(err) {
		t.Errorf("expected jdk-21 prefix to be stripped, but found it")
	}

	// Nested file contents match.
	got, err := os.ReadFile(filepath.Join(versionDir, "21", "lib", "whatever"))
	if err != nil {
		t.Fatalf("reading nested file: %v", err)
	}
	if string(got) != "data" {
		t.Errorf("nested file = %q, want %q", got, "data")
	}
}

// makeTarGzHeaders builds a tar.gz from explicit headers so callers control
// Typeflag (dir, reg, symlink).
func makeTarGzHeaders(t *testing.T, headers []*tar.Header, contents map[string]string) []byte {
	t.Helper()
	var buf bytes.Buffer
	gz := gzip.NewWriter(&buf)
	tw := tar.NewWriter(gz)
	for _, h := range headers {
		if h.Typeflag == tar.TypeReg {
			h.Size = int64(len(contents[h.Name]))
		}
		if err := tw.WriteHeader(h); err != nil {
			t.Fatalf("tar header %q: %v", h.Name, err)
		}
		if h.Typeflag == tar.TypeReg {
			if _, err := tw.Write([]byte(contents[h.Name])); err != nil {
				t.Fatalf("tar write %q: %v", h.Name, err)
			}
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

// TestExtractTarGz_dirsAndSymlink calls extractTarGz directly on a crafted
// archive exercising TypeDir, TypeReg, and TypeSymlink branches, plus top-level
// stripping.
func TestExtractTarGz_dirsAndSymlink(t *testing.T) {
	headers := []*tar.Header{
		{Name: "jdk-21/emptydir/", Typeflag: tar.TypeDir, Mode: 0o755},
		{Name: "jdk-21/lib/jli/java", Typeflag: tar.TypeReg, Mode: 0o755},
		{Name: "jdk-21/bin/java", Typeflag: tar.TypeSymlink, Linkname: "../lib/jli/java", Mode: 0o777},
	}
	contents := map[string]string{"jdk-21/lib/jli/java": "real-binary"}
	archive := makeTarGzHeaders(t, headers, contents)

	dest := t.TempDir()
	srcPath := filepath.Join(t.TempDir(), "jre.tar.gz")
	if err := os.WriteFile(srcPath, archive, 0o644); err != nil {
		t.Fatalf("writing archive: %v", err)
	}

	d := NewDownloader()
	if err := d.extractTarGz(srcPath, dest); err != nil {
		t.Fatalf("extractTarGz: %v", err)
	}

	// Empty dir exists with top-level component stripped (TypeDir branch).
	info, err := os.Stat(filepath.Join(dest, "emptydir"))
	if err != nil {
		t.Fatalf("stat emptydir: %v", err)
	}
	if !info.IsDir() {
		t.Errorf("emptydir is not a directory")
	}
	// jdk-21 prefix stripped.
	if _, err := os.Stat(filepath.Join(dest, "jdk-21")); !os.IsNotExist(err) {
		t.Errorf("expected jdk-21 prefix to be stripped, but found it")
	}
	// Regular file extracted and stripped.
	if got, err := os.ReadFile(filepath.Join(dest, "lib", "jli", "java")); err != nil {
		t.Fatalf("reading reg file: %v", err)
	} else if string(got) != "real-binary" {
		t.Errorf("reg file = %q, want %q", got, "real-binary")
	}

	if runtime.GOOS == "windows" {
		t.Skip("symlinks need privilege on windows; os.Symlink error is swallowed")
	}
	link := filepath.Join(dest, "bin", "java")
	target, err := os.Readlink(link)
	if err != nil {
		t.Fatalf("readlink: %v", err)
	}
	if target != "../lib/jli/java" {
		t.Errorf("symlink target = %q, want %q", target, "../lib/jli/java")
	}
}

func TestFindJavaExecutable(t *testing.T) {
	javaName := "java"
	if runtime.GOOS == "windows" {
		javaName = "java.exe"
	}

	tests := []struct {
		name    string
		setup   func(t *testing.T, dir string) // creates the tree
		wantRel string                         // expected returned path relative to dir; "" means error expected
	}{
		{
			name: "bin/java present",
			setup: func(t *testing.T, dir string) {
				binDir := filepath.Join(dir, "bin")
				if err := os.MkdirAll(binDir, 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(filepath.Join(binDir, javaName), []byte("x"), 0o755); err != nil {
					t.Fatal(err)
				}
			},
			wantRel: filepath.Join("bin", javaName),
		},
		{
			name: "java not in bin folder",
			setup: func(t *testing.T, dir string) {
				libDir := filepath.Join(dir, "lib")
				if err := os.MkdirAll(libDir, 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(filepath.Join(libDir, javaName), []byte("x"), 0o755); err != nil {
					t.Fatal(err)
				}
			},
			wantRel: "", // not under bin/ -> not-found error
		},
		{
			name:    "empty dir",
			setup:   func(t *testing.T, dir string) {},
			wantRel: "",
		},
	}

	d := NewDownloader()
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			dir := t.TempDir()
			tc.setup(t, dir)
			got, err := d.FindJavaExecutable(dir)
			if tc.wantRel == "" {
				if err == nil {
					t.Fatalf("expected error, got path %q", got)
				}
				return
			}
			if err != nil {
				t.Fatalf("FindJavaExecutable: %v", err)
			}
			want := filepath.Join(dir, tc.wantRel)
			if got != want {
				t.Errorf("path = %q, want %q", got, want)
			}
		})
	}
}
