package launch

import (
	"context"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
)

func TestLauncher_IsFullyDownloaded(t *testing.T) {
	// Setup
	tmpDir, err := os.MkdirTemp("", "mctui-launch-test")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpDir)

	// Create a dummy instance
	inst := &core.Instance{
		ID:      "test-inst",
		Name:    "Test Instance",
		Path:    tmpDir,
		Version: "1.21.4",
	}

	// Create dummy version info with a library that definitely doesn't exist
	version := &core.VersionDetails{
		ID: "1.21.4",
		Libraries: []core.Library{
			{
				Name: "com.example:missing:1.0.0",
				Downloads: &core.LibraryDownloads{
					Artifact: &core.Artifact{
						Path: "missing.jar",
						URL:  "http://localhost:0/missing.jar", // invalid URL
						Size: 100,
						SHA1: "0000",
					},
				},
			},
		},
	}

	cfg := &config.Config{
		DataDir:      tmpDir,
		LibrariesDir: tmpDir, // Use tmp dir so it checks there
	}

	t.Run("AlreadyDownloaded_True", func(t *testing.T) {
		inst.IsFullyDownloaded = true

		// Both the library and the asset index point at unroutable URLs, so any
		// download attempt fails. JavaPath is set up front so checkJava never runs
		// real detection or an Adoptium download.
		v := *version
		v.AssetIndex = core.AssetIndexRef{
			ID:  "test-assets",
			URL: "http://localhost:0/assets.json",
		}
		l := NewLauncher(&Options{
			Instance:    inst,
			VersionInfo: &v,
			Config:      cfg,
			JavaPath:    "dummy-java",
		}, nil)
		l.retryMax = 0 // fail fast if a download is wrongly attempted

		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()

		// The dummy java can't be exec'd, so Launch must fail — but only at the
		// final "Launching" step, proving both download steps were skipped.
		err := l.Launch(ctx)
		if err == nil {
			t.Fatal("expected Launch to fail exec'ing the dummy java")
		}
		if !strings.HasPrefix(err.Error(), "Launching:") {
			t.Errorf("expected failure at the Launching step (downloads skipped), got: %v", err)
		}
	})

	t.Run("AlreadyDownloaded_False", func(t *testing.T) {
		inst.IsFullyDownloaded = false
		l := NewLauncher(&Options{
			Instance:    inst,
			VersionInfo: version,
			Config:      cfg,
			JavaPath:    "dummy-java",
		}, nil)
		l.retryMax = 0 // fail fast instead of retryablehttp backoff against the bad URL

		// Should try to download and fail because of bad URL
		err := l.Launch(context.Background())

		if err == nil {
			t.Error("Expected download error, got success")
		} else {
			// Error should be about download
			// Since we have bad URL, it might be dial error or similar
			// check for "Download libraries" in error context if wrapped
			// Launch wraps errors: fmt.Errorf("%s: %w", step.name, err)
			if !strings.Contains(err.Error(), "Downloading libraries") {
				t.Errorf("Expected 'Downloading libraries' error, got: %v", err)
			}
		}
	})
}
