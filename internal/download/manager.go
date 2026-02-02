// Package download handles parallel file downloads with progress tracking.
package download

import (
	"context"
	"crypto/sha1"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"time"

	"github.com/dustin/go-humanize"
	"github.com/hashicorp/go-retryablehttp"
)

// Item represents a single download item
type Item struct {
	URL      string
	Path     string // Local destination path
	SHA1     string // Expected SHA1 hash (optional)
	Size     int64  // Expected size in bytes
	Priority int    // Higher = download first
}

// Progress tracks download progress
type Progress struct {
	TotalBytes      int64
	DownloadedBytes int64
	TotalItems      int
	CompletedItems  int
	CurrentItem     string
	Speed           float64 // bytes per second
}

// Manager handles parallel downloads
type Manager struct {
	httpClient  *http.Client
	workerCount int

	// Progress tracking
	mu              sync.RWMutex
	progress        Progress
	downloadedBytes int64
}

// NewManager creates a new download manager
func NewManager(workerCount int) *Manager {
	if workerCount <= 0 {
		workerCount = 4
	}

	// Create retryable client with sensible defaults
	retryClient := retryablehttp.NewClient()
	retryClient.RetryMax = 3
	retryClient.RetryWaitMin = 1 * time.Second
	retryClient.RetryWaitMax = 10 * time.Second
	retryClient.Logger = nil // Silence default logging

	// Configure underlying transport
	retryClient.HTTPClient.Transport = &http.Transport{
		MaxIdleConns:        100,
		MaxIdleConnsPerHost: 10,
		IdleConnTimeout:     90 * time.Second,
	}
	retryClient.HTTPClient.Timeout = 5 * time.Minute

	return &Manager{
		httpClient:  retryClient.StandardClient(),
		workerCount: workerCount,
	}
}

// Result contains the outcome of a download batch
type Result struct {
	Completed int
	Failed    int
	Errors    []error
}

// Download downloads all items and returns progress on the channel
func (m *Manager) Download(ctx context.Context, items []Item, progressChan chan<- Progress) (*Result, error) {
	if len(items) == 0 {
		return &Result{}, nil
	}

	// Calculate total size
	var totalSize int64
	for _, item := range items {
		totalSize += item.Size
	}

	m.mu.Lock()
	m.progress = Progress{
		TotalBytes: totalSize,
		TotalItems: len(items),
	}
	m.downloadedBytes = 0
	m.mu.Unlock()

	// Create work channel
	workChan := make(chan Item, len(items))
	for _, item := range items {
		workChan <- item
	}
	close(workChan)

	// Results collection
	var (
		completed int64
		failed    int64
		errMu     sync.Mutex
		errors    []error
	)

	// Signal for shutting down progress reporter
	doneSignal := make(chan struct{})

	// Start progress reporter (only if channel provided)
	progressDone := make(chan struct{})
	if progressChan != nil {
		go func() {
			defer close(progressDone)
			ticker := time.NewTicker(100 * time.Millisecond)
			defer ticker.Stop()

			var lastBytes int64
			lastTime := time.Now()

			for {
				select {
				case <-ctx.Done():
					return
				case <-doneSignal:
					return
				case <-ticker.C:
					m.mu.RLock()
					p := m.progress
					currentBytes := atomic.LoadInt64(&m.downloadedBytes)
					m.mu.RUnlock()

					// Calculate speed
					now := time.Now()
					elapsed := now.Sub(lastTime).Seconds()
					if elapsed > 0 {
						p.Speed = float64(currentBytes-lastBytes) / elapsed
						lastBytes = currentBytes
						lastTime = now
					}
					p.DownloadedBytes = currentBytes
					p.CompletedItems = int(atomic.LoadInt64(&completed))

					select {
					case progressChan <- p:
					default:
					}
				}
			}
		}()
	} else {
		close(progressDone)
	}

	// Start workers
	var wg sync.WaitGroup
	for i := 0; i < m.workerCount; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for item := range workChan {
				select {
				case <-ctx.Done():
					return
				default:
				}

				m.mu.Lock()
				m.progress.CurrentItem = filepath.Base(item.Path)
				m.mu.Unlock()

				if err := m.downloadItem(ctx, item); err != nil {
					atomic.AddInt64(&failed, 1)
					errMu.Lock()
					errors = append(errors, fmt.Errorf("%s: %w", item.URL, err))
					errMu.Unlock()
				} else {
					atomic.AddInt64(&completed, 1)
				}
			}
		}()
	}

	wg.Wait()
	close(doneSignal)
	<-progressDone

	return &Result{
		Completed: int(completed),
		Failed:    int(failed),
		Errors:    errors,
	}, nil
}

// downloadItem downloads a single item
func (m *Manager) downloadItem(ctx context.Context, item Item) error {
	// Check if file already exists with correct hash
	if item.SHA1 != "" {
		if hash, err := hashFile(item.Path); err == nil && hash == item.SHA1 {
			atomic.AddInt64(&m.downloadedBytes, item.Size)
			return nil // Already downloaded
		}
	}

	// Ensure directory exists
	if err := os.MkdirAll(filepath.Dir(item.Path), 0755); err != nil {
		return fmt.Errorf("creating directory: %w", err)
	}

	// Create request
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, item.URL, nil)
	if err != nil {
		return fmt.Errorf("creating request: %w", err)
	}

	// Execute request (retries handled by retryablehttp)
	resp, err := m.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("downloading: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	// Create temp file
	tmpPath := item.Path + ".tmp"
	f, err := os.Create(tmpPath)
	if err != nil {
		return fmt.Errorf("creating file: %w", err)
	}

	// Download with progress tracking
	hasher := sha1.New()
	writer := io.MultiWriter(f, hasher)

	buf := make([]byte, 32*1024)
	for {
		n, readErr := resp.Body.Read(buf)
		if n > 0 {
			if _, writeErr := writer.Write(buf[:n]); writeErr != nil {
				f.Close()
				os.Remove(tmpPath)
				return fmt.Errorf("writing file: %w", writeErr)
			}
			atomic.AddInt64(&m.downloadedBytes, int64(n))
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			f.Close()
			os.Remove(tmpPath)
			return fmt.Errorf("reading response: %w", readErr)
		}
	}

	if err := f.Close(); err != nil {
		os.Remove(tmpPath)
		return fmt.Errorf("closing file: %w", err)
	}

	// Verify hash
	if item.SHA1 != "" {
		hash := hex.EncodeToString(hasher.Sum(nil))
		if hash != item.SHA1 {
			os.Remove(tmpPath)
			return fmt.Errorf("hash mismatch: expected %s, got %s", item.SHA1, hash)
		}
	}

	// Move to final location
	if err := os.Rename(tmpPath, item.Path); err != nil {
		os.Remove(tmpPath)
		return fmt.Errorf("renaming file: %w", err)
	}

	return nil
}

// hashFile computes SHA1 of a file
func hashFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()

	h := sha1.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}

	return hex.EncodeToString(h.Sum(nil)), nil
}

// FormatSpeed formats download speed for display
func FormatSpeed(bytesPerSec float64) string {
	return humanize.Bytes(uint64(bytesPerSec)) + "/s"
}
