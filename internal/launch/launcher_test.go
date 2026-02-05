package launch

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/quasar/mctui/internal/config"
	"github.com/quasar/mctui/internal/core"
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
		ID:        "test-inst",
		Name:      "Test Instance",
		Path:      tmpDir,
		Version:   "1.21.4",
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
		
		l := NewLauncher(&Options{
			Instance:    inst,
			VersionInfo: version,
			Config:      cfg,
		}, nil)

		// Should skip download and return nil (success)
		// even though the library file is missing and URL is bad.
		err := l.Launch(context.Background())
		// checkJava might fail if not found, but we want to test downloadLibraries specifically.
		// Since Launch calls checkJava first, we might hit that.
		// Let's call downloadLibraries directly via a test-specific export or just rely on Launch failure point.
		// Launcher doesn't export downloadLibraries.
		// But verify behaviour via log/status if possible?
		// Or assume checkJava passes if we provide a dummy java path.
		
		l.opts.JavaPath = "dummy-java" // Skip checkJava

		// However, downloadLibraries is step 2.
		// If checkJava passes, downloadLibraries is next.
		// If it skips, it goes to downloadAssets.
		// Assets also has items? VersionInfo has AssetIndex.
		// If we leave AssetIndex empty or dummy, it might fail there if not skipped.
		// We want verification that it SKIPPED.
		
		// Let's set the AssetIndex URL to bad too.
		version.AssetIndex = core.AssetIndexRef{
			ID:  "test-assets",
			URL: "http://localhost:0/assets.json",
		}

		ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
		defer cancel()

		// Launch should proceed comfortably through "Downloading libraries" and "Downloading assets"
		// because of the flag.
		// It will eventually fail at "Launching" because Java path is dummy or game jar missing.
		// But if it fails at "Downloading ...", then the flag didn't work.
		
		err = l.Launch(ctx)
		
		// We expect error at "Launching" or "Preparing game", NOT download.
		// "Preparing game" makes dirs. "Launching" fails exec.
		
		if err == nil {
			// It shouldn't succeed fully with dummy java
		} else {
			// Check error message.
			errStr := err.Error()
			if contains(errStr, "downloading libraries") || contains(errStr, "downloading assets") || contains(errStr, "dial tcp") {
				t.Errorf("Launcher tried to download despite IsFullyDownloaded=true: %v", err)
			}
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

		// Should try to download and fail because of bad URL
		err := l.Launch(context.Background())
		
		if err == nil {
			t.Error("Expected download error, got success")
		} else {
			// Error should be about download
			// Since we have bad URL, it might be dial error or similar
			// check for "Download libraries" in error context if wrapped
			// Launch wraps errors: fmt.Errorf("%s: %w", step.name, err)
			if !contains(err.Error(), "Downloading libraries") {
				t.Errorf("Expected 'Downloading libraries' error, got: %v", err)
			}
		}
	})
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && 
		(s[0:len(substr)] == substr || contains(s[1:], substr))
}
