package java

import (
	"archive/tar"
	"archive/zip"
	"compress/gzip"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/hashicorp/go-retryablehttp"
)

// Downloader handles downloading Java runtimes from Adoptium
type Downloader struct {
	client *retryablehttp.Client
}

// NewDownloader creates a new Java downloader
func NewDownloader() *Downloader {
	client := retryablehttp.NewClient()
	client.Logger = nil // specific logger can be added if needed
	return &Downloader{
		client: client,
	}
}

// DownloadRuntime downloads and extracts the requested Java version
// Returns the path to the java executable
func (d *Downloader) DownloadRuntime(ctx context.Context, version int, destBaseDir string, progressCb func(string)) (string, error) {
	// 1. Resolve URL
	progressCb(fmt.Sprintf("Resolving Java %d...", version))
	downloadURL, filename, err := d.resolveAdoptiumURL(ctx, version)
	if err != nil {
		return "", fmt.Errorf("resolving java version: %w", err)
	}

	// 2. Prepare paths
	versionDir := filepath.Join(destBaseDir, fmt.Sprintf("%d", version))
	if err := os.MkdirAll(versionDir, 0755); err != nil {
		return "", fmt.Errorf("creating dir: %w", err)
	}

	downloadPath := filepath.Join(versionDir, filename)

	// 3. Download
	progressCb(fmt.Sprintf("Downloading Java %d...", version))
	if err := d.downloadFile(ctx, downloadURL, downloadPath); err != nil {
		return "", fmt.Errorf("downloading file: %w", err)
	}
	defer os.Remove(downloadPath) // Clean up archive

	// 4. Extract
	progressCb("Extracting Java runtime...")
	if err := d.extractArchive(downloadPath, versionDir); err != nil {
		return "", fmt.Errorf("extracting archive: %w", err)
	}

	// 5. Find executable
	return d.FindJavaExecutable(versionDir)
}

func (d *Downloader) resolveAdoptiumURL(ctx context.Context, version int) (string, string, error) {
	osName := runtime.GOOS
	if osName == "darwin" {
		osName = "mac"
	}

	arch := runtime.GOARCH
	if arch == "amd64" {
		arch = "x64"
	} else if arch == "arm64" {
		arch = "aarch64"
	}

	url := fmt.Sprintf("https://api.adoptium.net/v3/assets/feature_releases/%d/ga?architecture=%s&heap_size=normal&image_type=jre&jvm_impl=hotspot&os=%s&page=0&page_size=1&project=jdk&sort_method=DEFAULT&sort_order=DESC&vendor=eclipse", version, arch, osName)

	req, err := retryablehttp.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return "", "", err
	}

	resp, err := d.client.Do(req)
	if err != nil {
		return "", "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		return "", "", fmt.Errorf("api returned status %d", resp.StatusCode)
	}

	var releases []interface{}
	if err := json.NewDecoder(resp.Body).Decode(&releases); err != nil {
		return "", "", err
	}

	if len(releases) == 0 {
		return "", "", fmt.Errorf("no releases found for java %d on %s/%s", version, osName, arch)
	}

	// Extract URL and Filename
	// Structure: [ { binaries: [ { package: { link: "...", name: "..." } } ] } ]
	rel := releases[0].(map[string]interface{})
	binaries := rel["binaries"].([]interface{})
	if len(binaries) == 0 {
		return "", "", fmt.Errorf("no binaries in release")
	}
	binary := binaries[0].(map[string]interface{})
	pkg := binary["package"].(map[string]interface{})

	link, _ := pkg["link"].(string)
	name, _ := pkg["name"].(string)

	return link, name, nil
}

func (d *Downloader) downloadFile(ctx context.Context, url, dest string) error {
	req, err := retryablehttp.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return err
	}

	resp, err := d.client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	f, err := os.Create(dest)
	if err != nil {
		return err
	}
	defer f.Close()

	// Just a simple copy for now, could add progress tracking wrapper if needed
	_, err = io.Copy(f, resp.Body)
	return err
}

func (d *Downloader) extractArchive(src, dest string) error {
	if strings.HasSuffix(src, ".zip") {
		return d.extractZip(src, dest)
	}
	return d.extractTarGz(src, dest)
}

func (d *Downloader) extractTarGz(src, dest string) error {
	f, err := os.Open(src)
	if err != nil {
		return err
	}
	defer f.Close()

	gzr, err := gzip.NewReader(f)
	if err != nil {
		return err
	}
	defer gzr.Close()

	tr := tar.NewReader(gzr)

	// We strip the top-level directory to keep things clean
	// Common loop
	for {
		header, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return err
		}

		// Strip first component: jdk-21.0.4/... -> ...
		parts := strings.Split(header.Name, string(os.PathSeparator))
		if len(parts) <= 1 {
			continue
		}
		// logic to strip top folder usually:
		relPath := strings.Join(parts[1:], string(os.PathSeparator))
		if relPath == "" {
			continue
		}

		target := filepath.Join(dest, relPath)

		switch header.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(target, 0755); err != nil {
				return err
			}
		case tar.TypeReg:
			if err := os.MkdirAll(filepath.Dir(target), 0755); err != nil {
				return err
			}
			f, err := os.OpenFile(target, os.O_CREATE|os.O_RDWR, os.FileMode(header.Mode))
			if err != nil {
				return err
			}
			if _, err := io.Copy(f, tr); err != nil {
				f.Close()
				return err
			}
			f.Close()
		case tar.TypeSymlink:
			// Handle symlinks on unix
			if err := os.MkdirAll(filepath.Dir(target), 0755); err != nil {
				return err
			}
			os.Symlink(header.Linkname, target)
		}
	}
	return nil
}

func (d *Downloader) extractZip(src, dest string) error {
	r, err := zip.OpenReader(src)
	if err != nil {
		return err
	}
	defer r.Close()

	for _, f := range r.File {
		// Strip first component logic
		parts := strings.Split(f.Name, "/") // zip uses forward slash
		if len(parts) <= 1 {
			continue
		}
		relPath := strings.Join(parts[1:], string(os.PathSeparator))
		if relPath == "" {
			continue
		}

		target := filepath.Join(dest, relPath)

		if f.FileInfo().IsDir() {
			os.MkdirAll(target, 0755)
			continue
		}

		if err := os.MkdirAll(filepath.Dir(target), 0755); err != nil {
			return err
		}

		outFile, err := os.OpenFile(target, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, f.Mode())
		if err != nil {
			return err
		}
		rc, err := f.Open()
		if err != nil {
			outFile.Close()
			return err
		}
		_, err = io.Copy(outFile, rc)
		outFile.Close()
		rc.Close()
	}
	return nil
}

func (d *Downloader) FindJavaExecutable(dir string) (string, error) {
	// Look for bin/java or bin/java.exe
	binName := "java"
	if runtime.GOOS == "windows" {
		binName = "java.exe"
	}

	var foundPath string
	_ = filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}
		if foundPath != "" {
			return filepath.SkipDir
		}
		if info.Name() == binName {
			// Check if it's in a bin folder to avoid other java files
			if filepath.Base(filepath.Dir(path)) == "bin" {
				foundPath = path
				return filepath.SkipDir
			}
		}
		return nil
	})

	if foundPath != "" {
		return foundPath, nil
	}
	return "", fmt.Errorf("java executable not found in %s", dir)
}
