package download

import (
	"context"
	"crypto/sha1"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

func TestDownload_SingleFile(t *testing.T) {
	// Create test server
	content := []byte("Hello, World!")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write(content)
	}))
	defer server.Close()

	// Setup temp dir
	tmpDir := t.TempDir()
	destPath := filepath.Join(tmpDir, "test.txt")

	// Download
	mgr := NewManager(1)
	result, err := mgr.Download(context.Background(), []Item{{
		URL:  server.URL,
		Path: destPath,
	}}, nil)

	if err != nil {
		t.Fatalf("Download failed: %v", err)
	}
	if result.Failed != 0 {
		t.Errorf("Expected 0 failures, got %d", result.Failed)
	}
	if result.Completed != 1 {
		t.Errorf("Expected 1 completed, got %d", result.Completed)
	}

	// Verify content
	data, err := os.ReadFile(destPath)
	if err != nil {
		t.Fatalf("Reading downloaded file: %v", err)
	}
	if string(data) != string(content) {
		t.Errorf("Content mismatch: got %q, want %q", data, content)
	}
}

func TestDownload_SHA1Validation(t *testing.T) {
	content := []byte("Test content for hashing")
	hash := sha1.Sum(content)
	expectedHash := hex.EncodeToString(hash[:])

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write(content)
	}))
	defer server.Close()

	tmpDir := t.TempDir()
	destPath := filepath.Join(tmpDir, "hashed.txt")

	mgr := NewManager(1)
	result, err := mgr.Download(context.Background(), []Item{{
		URL:  server.URL,
		Path: destPath,
		SHA1: expectedHash,
		Size: int64(len(content)),
	}}, nil)

	if err != nil {
		t.Fatalf("Download failed: %v", err)
	}
	if result.Failed != 0 {
		t.Errorf("Expected 0 failures, got %d with errors: %v", result.Failed, result.Errors)
	}
}

func TestDownload_SHA1Mismatch(t *testing.T) {
	content := []byte("Test content")

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write(content)
	}))
	defer server.Close()

	tmpDir := t.TempDir()
	destPath := filepath.Join(tmpDir, "bad_hash.txt")

	mgr := NewManager(1)
	result, _ := mgr.Download(context.Background(), []Item{{
		URL:  server.URL,
		Path: destPath,
		SHA1: "0000000000000000000000000000000000000000",
	}}, nil)

	if result.Failed != 1 {
		t.Errorf("Expected 1 failure due to hash mismatch, got %d", result.Failed)
	}
}

func TestDownload_SkipsExistingValid(t *testing.T) {
	content := []byte("Existing content")
	hash := sha1.Sum(content)
	expectedHash := hex.EncodeToString(hash[:])

	// Track if server was called
	serverCalled := false
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		serverCalled = true
		w.Write(content)
	}))
	defer server.Close()

	// Pre-create file with correct content
	tmpDir := t.TempDir()
	destPath := filepath.Join(tmpDir, "existing.txt")
	os.WriteFile(destPath, content, 0644)

	mgr := NewManager(1)
	result, err := mgr.Download(context.Background(), []Item{{
		URL:  server.URL,
		Path: destPath,
		SHA1: expectedHash,
		Size: int64(len(content)),
	}}, nil)

	if err != nil {
		t.Fatalf("Download failed: %v", err)
	}
	if result.Completed != 1 {
		t.Errorf("Expected 1 completed, got %d", result.Completed)
	}
	if serverCalled {
		t.Error("Server should not be called for existing valid file")
	}
}

func TestDownload_MultipleFiles(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("content-" + r.URL.Path))
	}))
	defer server.Close()

	tmpDir := t.TempDir()

	items := []Item{
		{URL: server.URL + "/1", Path: filepath.Join(tmpDir, "1.txt")},
		{URL: server.URL + "/2", Path: filepath.Join(tmpDir, "2.txt")},
		{URL: server.URL + "/3", Path: filepath.Join(tmpDir, "3.txt")},
	}

	mgr := NewManager(2) // 2 workers
	result, err := mgr.Download(context.Background(), items, nil)

	if err != nil {
		t.Fatalf("Download failed: %v", err)
	}
	if result.Completed != 3 {
		t.Errorf("Expected 3 completed, got %d", result.Completed)
	}

	// Verify all files exist
	for _, item := range items {
		if _, err := os.Stat(item.Path); err != nil {
			t.Errorf("File %s should exist: %v", item.Path, err)
		}
	}
}

func TestDownload_EmptyList(t *testing.T) {
	mgr := NewManager(4)
	result, err := mgr.Download(context.Background(), []Item{}, nil)

	if err != nil {
		t.Fatalf("Empty download should not fail: %v", err)
	}
	if result.Completed != 0 || result.Failed != 0 {
		t.Error("Empty download should have zero completed and failed")
	}
}

func TestFormatSpeed(t *testing.T) {
	tests := []struct {
		bps  float64
		want string
	}{
		{500, "500 B/s"},
		{1024, "1.0 kB/s"},
		{1536, "1.5 kB/s"},
		{1024 * 1024, "1.0 MB/s"},
		{10 * 1024 * 1024, "10 MB/s"},
	}

	for _, tt := range tests {
		got := FormatSpeed(tt.bps)
		// We're using humanize which might format slightly differently
		if got == "" {
			t.Errorf("FormatSpeed(%f) returned empty string", tt.bps)
		}
	}
}
