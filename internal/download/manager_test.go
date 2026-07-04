package download

import (
	"bytes"
	"context"
	"crypto/sha1"
	"encoding/hex"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
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

// TestDownload_TmpCleanupOnHashMismatch verifies the partial-write contract: when
// the downloaded bytes fail hash verification, neither the destination nor its
// .tmp staging file is left behind.
func TestDownload_TmpCleanupOnHashMismatch(t *testing.T) {
	content := []byte("content that will not match the expected hash")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write(content)
	}))
	defer server.Close()

	tmpDir := t.TempDir()
	destPath := filepath.Join(tmpDir, "f.bin")

	mgr := NewManagerWithRetryMax(1, 0)
	result, _ := mgr.Download(context.Background(), []Item{{
		URL:  server.URL,
		Path: destPath,
		SHA1: "0000000000000000000000000000000000000000",
	}}, nil)

	if result.Failed != 1 {
		t.Fatalf("Failed = %d, want 1", result.Failed)
	}
	if _, err := os.Stat(destPath); !os.IsNotExist(err) {
		t.Errorf("destination should not exist after hash mismatch (err=%v)", err)
	}
	if _, err := os.Stat(destPath + ".tmp"); !os.IsNotExist(err) {
		t.Errorf(".tmp file should be cleaned up after hash mismatch (err=%v)", err)
	}
}

// TestDownload_Non200 verifies a non-200 response is a clean failure carrying the
// status in the error (and is not retried into multi-second backoff via retryMax=0).
func TestDownload_Non200(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
	}))
	defer server.Close()

	tmpDir := t.TempDir()
	mgr := NewManagerWithRetryMax(1, 0)
	result, _ := mgr.Download(context.Background(), []Item{{
		URL:  server.URL,
		Path: filepath.Join(tmpDir, "x"),
	}}, nil)

	if result.Failed != 1 {
		t.Fatalf("Failed = %d, want 1", result.Failed)
	}
	if len(result.Errors) == 0 || !strings.Contains(result.Errors[0].Error(), "unexpected status: 404") {
		t.Errorf("error = %v, want one containing 'unexpected status: 404'", result.Errors)
	}
}

// TestDownload_ContextCancellation verifies an in-flight download that is cancelled
// fails cleanly without writing a partial file.
func TestDownload_ContextCancellation(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		<-r.Context().Done() // block until the client cancels the request
	}))
	defer server.Close()

	tmpDir := t.TempDir()
	destPath := filepath.Join(tmpDir, "x")

	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()

	mgr := NewManagerWithRetryMax(1, 0)
	result, _ := mgr.Download(ctx, []Item{{URL: server.URL, Path: destPath}}, nil)

	if result.Completed != 0 {
		t.Errorf("Completed = %d, want 0", result.Completed)
	}
	if result.Failed != 1 {
		t.Errorf("Failed = %d, want 1", result.Failed)
	}
	// Assert the failure is actually the cancellation, not some unrelated error —
	// otherwise this would pass even if cancel handling were broken.
	if len(result.Errors) == 0 || !errors.Is(result.Errors[0], context.DeadlineExceeded) {
		t.Errorf("error = %v, want one wrapping context.DeadlineExceeded", result.Errors)
	}
	if _, err := os.Stat(destPath); !os.IsNotExist(err) {
		t.Errorf("no file should be written on cancellation (err=%v)", err)
	}
}

// TestDownload_ProgressChannel exercises the progress-reporter goroutine: a slow,
// chunked response spans several reporter ticks so at least one progress update
// reports bytes in flight.
func TestDownload_ProgressChannel(t *testing.T) {
	const chunks = 20
	chunk := bytes.Repeat([]byte("x"), 8*1024)
	bodyLen := int64(chunks * len(chunk))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		flusher, _ := w.(http.Flusher)
		for i := 0; i < chunks; i++ {
			w.Write(chunk)
			if flusher != nil {
				flusher.Flush()
			}
			time.Sleep(25 * time.Millisecond) // ~500ms total, spanning several 100ms ticks
		}
	}))
	defer server.Close()

	tmpDir := t.TempDir()
	progressChan := make(chan Progress, 1024)

	mgr := NewManager(1)
	result, err := mgr.Download(context.Background(), []Item{{
		URL:  server.URL,
		Path: filepath.Join(tmpDir, "big.bin"),
	}}, progressChan)
	if err != nil {
		t.Fatalf("Download: %v", err)
	}
	close(progressChan) // reporter has exited by the time Download returns

	if result.Completed != 1 {
		t.Fatalf("Completed = %d, want 1", result.Completed)
	}
	// Require a genuinely MID-FLIGHT progress update (0 < bytes < total), not just
	// a non-zero reading at completion — otherwise the test wouldn't prove progress
	// is reported during the download.
	var sawMidFlight bool
	for p := range progressChan {
		if p.DownloadedBytes > 0 && p.DownloadedBytes < bodyLen {
			sawMidFlight = true
		}
	}
	if !sawMidFlight {
		t.Error("expected a mid-flight progress update (0 < DownloadedBytes < total)")
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
		if got := FormatSpeed(tt.bps); got != tt.want {
			t.Errorf("FormatSpeed(%v) = %q, want %q", tt.bps, got, tt.want)
		}
	}
}
