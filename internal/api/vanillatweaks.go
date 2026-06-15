// Package api Vanilla Tweaks client.
// Fetches the per-version resource-pack catalog and builds merged resource-pack
// zips via the public Vanilla Tweaks endpoints.
package api

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

const (
	// vanillaTweaksBaseURL is the site root; download links returned by the build
	// endpoint are absolute paths under this host.
	vanillaTweaksBaseURL = "https://vanillatweaks.net"

	// vtCatalogPathFmt is the per-version resource-pack catalog endpoint.
	// Format with the normalized "major.minor" version (e.g. "1.21").
	vtCatalogPathFmt = "/assets/resources/json/%s/rpcategories.json"

	// vtZipEndpoint builds + merges the selected packs into one downloadable zip.
	vtZipEndpoint = "/assets/server/zipresourcepacks.php"
)

// vtCatalogVersions lists the "major.minor" MC versions Vanilla Tweaks publishes
// resource-pack catalogs for, most recent first. Verified live against the site
// (versions returning HTTP 200 for the rpcategories.json endpoint). VTCatalogVersions
// returns a copy of this; NormalizeVTVersion resolves an instance version to the
// nearest entry here.
var vtCatalogVersions = []string{
	"1.21",
	"1.20",
	"1.19",
	"1.18",
	"1.17",
	"1.16",
	"1.15",
	"1.14",
	"1.13",
	"1.12",
	"1.11",
	"1.8",
}

// VanillaTweaksClient handles Vanilla Tweaks API interactions.
type VanillaTweaksClient struct {
	httpClient *http.Client
	baseURL    string
}

// NewVanillaTweaksClient creates a new Vanilla Tweaks API client.
func NewVanillaTweaksClient() *VanillaTweaksClient {
	return NewVanillaTweaksClientWithBaseURL(vanillaTweaksBaseURL)
}

// NewVanillaTweaksClientWithBaseURL creates a Vanilla Tweaks client pointed at
// baseURL. Exists as a dependency-injection seam so tests can target an httptest
// server; NewVanillaTweaksClient delegates here with the production const.
func NewVanillaTweaksClientWithBaseURL(baseURL string) *VanillaTweaksClient {
	return &VanillaTweaksClient{
		httpClient: &http.Client{
			Timeout: 30 * time.Second,
		},
		baseURL: baseURL,
	}
}

// ResourcePackCatalog is the decoded rpcategories.json for a single MC version.
// Version records which catalog version was actually fetched (after nearest-match
// normalization) so the UI can surface it.
type ResourcePackCatalog struct {
	Version    string       `json:"-"`
	Categories []RPCategory `json:"categories"`
}

// RPCategory is one top-level (or nested) category in the catalog. The display
// name (Category) is the key used in the build endpoint's selection map.
type RPCategory struct {
	// Category is the human-readable display name (e.g. "Aesthetic"). This is the
	// key used when POSTing a selection to the build endpoint.
	Category string `json:"category"`
	// Categories holds nested subcategories (may be empty/absent).
	Categories []RPCategory `json:"categories,omitempty"`
	// Packs are the resource packs directly in this category.
	Packs []RPPack `json:"packs,omitempty"`
	// Warning is an optional category-level advisory (text + color).
	Warning *RPWarning `json:"warning,omitempty"`
}

// RPPack is a single resource pack within a category.
type RPPack struct {
	// Name is the stable pack id used in the build selection map.
	Name string `json:"name"`
	// Display is the human-readable label.
	Display string `json:"display"`
	// Description is HTML markup describing the pack.
	Description string `json:"description"`
	// Incompatible lists pack "name" ids that cannot be enabled alongside this one.
	Incompatible []string `json:"incompatible,omitempty"`
	// Experiment marks experimental packs.
	Experiment bool `json:"experiment,omitempty"`
	// PreviewExtension, when set (e.g. "gif"), means an animated preview exists at
	// the /previews/ path; otherwise only the static /icons/ .png is available.
	PreviewExtension string `json:"previewExtension,omitempty"`
	// Video is an external credit/preview URL (e.g. a creator's page), if any.
	Video string `json:"video,omitempty"`
	// VideoText is the label for Video (e.g. "Credit: Ninni").
	VideoText string `json:"videoText,omitempty"`
	// Priority flags featured packs (higher sorts first); 0 when unset.
	Priority int `json:"priority,omitempty"`
}

const (
	vtPreviewPathFmt = "/assets/resources/previews/resourcepacks/%s/%s.%s"
	vtIconPathFmt    = "/assets/resources/icons/resourcepacks/%s/%s.png"
)

// ResourcePackPreviewURL returns a browser-openable image URL for a pack: the
// animated preview when the pack declares a previewExtension, else the static
// icon (which exists for every pack). version is normalized to the published
// major.minor the assets are keyed by.
func ResourcePackPreviewURL(version string, p RPPack) string {
	resolved, _ := NormalizeVTVersion(version)
	if resolved == "" {
		// No usable version → no well-formed asset URL. Callers treat "" as
		// "no preview" rather than opening a broken link.
		return ""
	}
	name := url.PathEscape(p.Name)
	if p.PreviewExtension != "" {
		return vanillaTweaksBaseURL + fmt.Sprintf(vtPreviewPathFmt, resolved, name, p.PreviewExtension)
	}
	return vanillaTweaksBaseURL + fmt.Sprintf(vtIconPathFmt, resolved, name)
}

// RPWarning is a category-level advisory shown to the user.
type RPWarning struct {
	Text  string `json:"text"`
	Color string `json:"color"`
}

// vtBuildResponse is the decoded response from the zip build endpoint.
type vtBuildResponse struct {
	Status string `json:"status"`
	Link   string `json:"link"`
}

// FetchResourcePackCatalog fetches and decodes the resource-pack catalog for the
// given MC version. The version is normalized to the nearest published catalog
// version (see NormalizeVTVersion); the resolved version is recorded on the
// returned catalog's Version field.
func (c *VanillaTweaksClient) FetchResourcePackCatalog(ctx context.Context, version string) (*ResourcePackCatalog, error) {
	resolved, _ := NormalizeVTVersion(version)
	if resolved == "" {
		return nil, fmt.Errorf("no Vanilla Tweaks catalog for version %q", version)
	}

	reqURL := c.baseURL + fmt.Sprintf(vtCatalogPathFmt, resolved)

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, reqURL, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}
	req.Header.Set("User-Agent", userAgent)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("fetching resource-pack catalog: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("no Vanilla Tweaks catalog published for version %s", resolved)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	var catalog ResourcePackCatalog
	if err := json.NewDecoder(resp.Body).Decode(&catalog); err != nil {
		return nil, fmt.Errorf("decoding catalog: %w", err)
	}
	catalog.Version = resolved

	return &catalog, nil
}

// BuildResourcePackZip POSTs the selection to the merge endpoint and returns the
// absolute download URL for the resulting merged resource-pack zip.
//
// selection maps category DISPLAY name -> []pack name ids, e.g.
// {"Aesthetic":["BorderlessGlass","ClearGlass"]}. version is the (already
// normalized) catalog version.
func (c *VanillaTweaksClient) BuildResourcePackZip(ctx context.Context, selection map[string][]string, version string) (downloadURL string, err error) {
	if len(selection) == 0 {
		return "", fmt.Errorf("empty selection")
	}

	resolved, _ := NormalizeVTVersion(version)
	if resolved == "" {
		return "", fmt.Errorf("no Vanilla Tweaks catalog for version %q", version)
	}

	packsJSON, err := json.Marshal(selection)
	if err != nil {
		return "", fmt.Errorf("encoding selection: %w", err)
	}

	form := url.Values{}
	form.Set("packs", string(packsJSON))
	form.Set("version", resolved)

	reqURL := c.baseURL + vtZipEndpoint
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, reqURL, strings.NewReader(form.Encode()))
	if err != nil {
		return "", fmt.Errorf("creating request: %w", err)
	}
	req.Header.Set("User-Agent", userAgent)
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return "", fmt.Errorf("building resource pack: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	var built vtBuildResponse
	if err := json.NewDecoder(resp.Body).Decode(&built); err != nil {
		return "", fmt.Errorf("decoding build response: %w", err)
	}

	if built.Status != "success" {
		return "", fmt.Errorf("build failed: status %q", built.Status)
	}
	if built.Link == "" {
		return "", fmt.Errorf("build succeeded but returned no download link")
	}

	// Link is a site-relative path (e.g. "/download/VanillaTweaks_rXXXXX_MC1.21.x.zip").
	// Return an absolute URL. Guard against the endpoint ever returning an absolute one.
	if strings.HasPrefix(built.Link, "http://") || strings.HasPrefix(built.Link, "https://") {
		return built.Link, nil
	}
	return c.baseURL + built.Link, nil
}

// DownloadResourcePackZip downloads the merged zip at downloadURL to dest. It is
// a thin convenience over the download manager for callers that already have a
// build URL.
func (c *VanillaTweaksClient) DownloadResourcePackZip(ctx context.Context, downloadURL, dest string) error {
	if downloadURL == "" {
		return fmt.Errorf("empty download URL")
	}

	// Ensure the destination directory exists; the screen can open before the
	// launcher has created the resourcepacks/ dir.
	if err := os.MkdirAll(filepath.Dir(dest), 0755); err != nil {
		return fmt.Errorf("creating directory: %w", err)
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, downloadURL, nil)
	if err != nil {
		return fmt.Errorf("creating request: %w", err)
	}
	req.Header.Set("User-Agent", userAgent)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("downloading: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	// Stream to a temp file, then atomically rename into place. Mirrors the
	// download manager's tmp+rename convention so a cancelled/failed download
	// never leaves a half-written pack at dest.
	tmpPath := dest + ".tmp"
	f, err := os.Create(tmpPath)
	if err != nil {
		return fmt.Errorf("creating file: %w", err)
	}

	if _, err := io.Copy(f, resp.Body); err != nil {
		f.Close()
		os.Remove(tmpPath)
		return fmt.Errorf("writing file: %w", err)
	}
	if err := f.Close(); err != nil {
		os.Remove(tmpPath)
		return fmt.Errorf("closing file: %w", err)
	}

	if err := os.Rename(tmpPath, dest); err != nil {
		os.Remove(tmpPath)
		return fmt.Errorf("renaming file: %w", err)
	}

	return nil
}

// BaseURL returns the configured site root (e.g. "https://vanillatweaks.net").
// Callers prepend it to relative links returned by the build endpoint.
func (c *VanillaTweaksClient) BaseURL() string {
	return c.baseURL
}

// NormalizeVTVersion maps an instance MC version (e.g. "1.21.4") to the nearest
// available Vanilla Tweaks catalog version (e.g. "1.21"). Returns the normalized
// "major.minor" string and whether a published catalog is expected to exist for
// it. When no mapping is possible, ok is false and the returned string is the
// best-effort major.minor.
func NormalizeVTVersion(mcVersion string) (normalized string, ok bool) {
	major, minor, parsed := parseMajorMinor(mcVersion)
	if !parsed {
		return "", false
	}
	candidate := fmt.Sprintf("%d.%d", major, minor)

	// Exact major.minor match wins.
	for _, v := range vtCatalogVersions {
		if v == candidate {
			return v, true
		}
	}

	// Otherwise pick the nearest published catalog that is <= the requested
	// version within the same major (e.g. "1.21.4" with only "1.20" published
	// resolves to "1.20"; a future "1.22" resolves to the highest "1.x"). This
	// keeps newer-than-published instances pinned to the latest available pack
	// set rather than failing outright.
	best := ""
	bestMajor, bestMinor := -1, -1
	for _, v := range vtCatalogVersions {
		vMaj, vMin, vOK := parseMajorMinor(v)
		if !vOK {
			continue
		}
		// Skip catalogs newer than what was requested.
		if vMaj > major || (vMaj == major && vMin > minor) {
			continue
		}
		if vMaj > bestMajor || (vMaj == bestMajor && vMin > bestMinor) {
			best, bestMajor, bestMinor = v, vMaj, vMin
		}
	}
	if best != "" {
		// ok reports whether a published catalog is expected; the resolved
		// version is a real published entry, so ok is true even though the
		// requested major.minor wasn't published verbatim.
		return best, true
	}

	// Requested version predates every published catalog: best-effort
	// major.minor, ok=false so callers can surface that no catalog exists.
	return candidate, false
}

// parseMajorMinor extracts the leading major and minor components of a Minecraft
// version string (e.g. "1.21.4" -> 1, 21). Snapshots and non-numeric versions
// (e.g. "23w13a") return parsed=false.
func parseMajorMinor(v string) (major, minor int, parsed bool) {
	v = strings.TrimSpace(v)
	if v == "" {
		return 0, 0, false
	}
	parts := strings.SplitN(v, ".", 3)
	if len(parts) < 2 {
		return 0, 0, false
	}
	maj, err := strconv.Atoi(parts[0])
	if err != nil {
		return 0, 0, false
	}
	min, err := strconv.Atoi(parts[1])
	if err != nil {
		return 0, 0, false
	}
	return maj, min, true
}

// VTCatalogVersions lists the MC catalog versions Vanilla Tweaks publishes, most
// recent first. Used by NormalizeVTVersion and surfaced for diagnostics.
func VTCatalogVersions() []string {
	out := make([]string, len(vtCatalogVersions))
	copy(out, vtCatalogVersions)
	return out
}
