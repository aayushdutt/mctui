// Package api Modrinth client.
// Handles mod, modpack, and resource pack searches.
package api

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"
)

const (
	modrinthBaseURL = "https://api.modrinth.com/v2"
	userAgent       = "quasar/mctui/1.0.0 (github.com/quasar/mctui)"
)

// ModrinthClient handles Modrinth API interactions
type ModrinthClient struct {
	httpClient *http.Client
	baseURL    string
}

// NewModrinthClient creates a new Modrinth API client
func NewModrinthClient() *ModrinthClient {
	return &ModrinthClient{
		httpClient: &http.Client{
			Timeout: 30 * time.Second,
		},
		baseURL: modrinthBaseURL,
	}
}

// Project represents a Modrinth project (mod, modpack, etc.)
type Project struct {
	ID           string   `json:"id"`
	Slug         string   `json:"slug"`
	ProjectType  string   `json:"project_type"` // mod, modpack, resourcepack, shader
	Team         string   `json:"team"`
	Title        string   `json:"title"`
	Description  string   `json:"description"`
	Body         string   `json:"body"` // Full description (markdown)
	Categories   []string `json:"categories"`
	ClientSide   string   `json:"client_side"` // required, optional, unsupported
	ServerSide   string   `json:"server_side"` // required, optional, unsupported
	Downloads    int      `json:"downloads"`
	Followers    int      `json:"followers"`
	IconURL      string   `json:"icon_url"`
	Published    string   `json:"published"`
	Updated      string   `json:"updated"`
	License      License  `json:"license"`
	Versions     []string `json:"versions"`      // Version IDs
	GameVersions []string `json:"game_versions"` // Supported MC versions
	Loaders      []string `json:"loaders"`       // Supported loaders
}

// License represents project license info
type License struct {
	ID   string `json:"id"`
	Name string `json:"name"`
	URL  string `json:"url"`
}

// ProjectVersion represents a specific version of a project
type ProjectVersion struct {
	ID            string        `json:"id"`
	ProjectID     string        `json:"project_id"`
	Name          string        `json:"name"`
	VersionNumber string        `json:"version_number"`
	Changelog     string        `json:"changelog"`
	Dependencies  []Dependency  `json:"dependencies"`
	GameVersions  []string      `json:"game_versions"`
	VersionType   string        `json:"version_type"` // release, beta, alpha
	Loaders       []string      `json:"loaders"`
	Featured      bool          `json:"featured"`
	Files         []VersionFile `json:"files"`
	Published     string        `json:"published"`
	Downloads     int           `json:"downloads"`
}

// Dependency represents a version dependency
type Dependency struct {
	VersionID      string `json:"version_id"`
	ProjectID      string `json:"project_id"`
	FileName       string `json:"file_name"`
	DependencyType string `json:"dependency_type"` // required, optional, incompatible, embedded
}

// VersionFile represents a downloadable file
type VersionFile struct {
	Hashes   FileHashes `json:"hashes"`
	URL      string     `json:"url"`
	Filename string     `json:"filename"`
	Primary  bool       `json:"primary"`
	Size     int64      `json:"size"`
	FileType string     `json:"file_type"` // required-resource-pack, optional-resource-pack, or null
}

// FileHashes contains file checksums
type FileHashes struct {
	SHA1   string `json:"sha1"`
	SHA512 string `json:"sha512"`
}

// SearchResult represents a search response
type SearchResult struct {
	Hits      []SearchHit `json:"hits"`
	Offset    int         `json:"offset"`
	Limit     int         `json:"limit"`
	TotalHits int         `json:"total_hits"`
}

// SearchHit represents a single search result
type SearchHit struct {
	ProjectID     string   `json:"project_id"`
	ProjectType   string   `json:"project_type"`
	Slug          string   `json:"slug"`
	Author        string   `json:"author"`
	Title         string   `json:"title"`
	Description   string   `json:"description"`
	Categories    []string `json:"categories"`
	DisplayCats   []string `json:"display_categories"`
	Versions      []string `json:"versions"`
	Downloads     int      `json:"downloads"`
	Follows       int      `json:"follows"`
	IconURL       string   `json:"icon_url"`
	DateCreated   string   `json:"date_created"`
	DateModified  string   `json:"date_modified"`
	LatestVersion string   `json:"latest_version"`
	License       string   `json:"license"`
	ClientSide    string   `json:"client_side"`
	ServerSide    string   `json:"server_side"`
	Gallery       []string `json:"gallery"`
	FeaturedGal   string   `json:"featured_gallery"`
	Color         int      `json:"color"`
}

// SearchOptions configures a search query
type SearchOptions struct {
	Query       string
	Facets      [][]string // Facet filters
	Index       string     // Sort index: relevance, downloads, follows, newest, updated
	Offset      int
	Limit       int
	Loaders     []string
	GameVersion string
	ProjectType string // mod, modpack, resourcepack, shader
}

// Search searches for projects
func (c *ModrinthClient) Search(ctx context.Context, opts SearchOptions) (*SearchResult, error) {
	params := url.Values{}

	if opts.Query != "" {
		params.Set("query", opts.Query)
	}
	if opts.Index != "" {
		params.Set("index", opts.Index)
	}
	if opts.Offset > 0 {
		params.Set("offset", fmt.Sprintf("%d", opts.Offset))
	}
	if opts.Limit > 0 {
		params.Set("limit", fmt.Sprintf("%d", opts.Limit))
	} else {
		params.Set("limit", "20")
	}

	// Build facets
	var facets [][]string
	if len(opts.Loaders) > 0 {
		loaderFacets := make([]string, len(opts.Loaders))
		for i, l := range opts.Loaders {
			loaderFacets[i] = fmt.Sprintf("categories:%s", l)
		}
		facets = append(facets, loaderFacets)
	}
	if opts.GameVersion != "" {
		facets = append(facets, []string{fmt.Sprintf("versions:%s", opts.GameVersion)})
	}
	if opts.ProjectType != "" {
		facets = append(facets, []string{fmt.Sprintf("project_type:%s", opts.ProjectType)})
	}
	facets = append(facets, opts.Facets...)

	if len(facets) > 0 {
		facetJSON, _ := json.Marshal(facets)
		params.Set("facets", string(facetJSON))
	}

	reqURL := fmt.Sprintf("%s/search?%s", c.baseURL, params.Encode())

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, reqURL, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}
	req.Header.Set("User-Agent", userAgent)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("searching: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	var result SearchResult
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	return &result, nil
}

// GetProject fetches a project by ID or slug
func (c *ModrinthClient) GetProject(ctx context.Context, idOrSlug string) (*Project, error) {
	reqURL := fmt.Sprintf("%s/project/%s", c.baseURL, url.PathEscape(idOrSlug))

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, reqURL, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}
	req.Header.Set("User-Agent", userAgent)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("fetching project: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("project not found: %s", idOrSlug)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	var project Project
	if err := json.NewDecoder(resp.Body).Decode(&project); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	return &project, nil
}

// GetProjectVersions fetches all versions of a project
func (c *ModrinthClient) GetProjectVersions(ctx context.Context, projectID string, loaders []string, gameVersions []string) ([]ProjectVersion, error) {
	params := url.Values{}
	if len(loaders) > 0 {
		loadersJSON, _ := json.Marshal(loaders)
		params.Set("loaders", string(loadersJSON))
	}
	if len(gameVersions) > 0 {
		versionsJSON, _ := json.Marshal(gameVersions)
		params.Set("game_versions", string(versionsJSON))
	}

	reqURL := fmt.Sprintf("%s/project/%s/version", c.baseURL, url.PathEscape(projectID))
	if len(params) > 0 {
		reqURL += "?" + params.Encode()
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, reqURL, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}
	req.Header.Set("User-Agent", userAgent)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("fetching versions: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	var versions []ProjectVersion
	if err := json.NewDecoder(resp.Body).Decode(&versions); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	return versions, nil
}

// GetVersion fetches a specific version
func (c *ModrinthClient) GetVersion(ctx context.Context, versionID string) (*ProjectVersion, error) {
	reqURL := fmt.Sprintf("%s/version/%s", c.baseURL, url.PathEscape(versionID))

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, reqURL, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}
	req.Header.Set("User-Agent", userAgent)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("fetching version: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("unexpected status: %d", resp.StatusCode)
	}

	var version ProjectVersion
	if err := json.NewDecoder(resp.Body).Decode(&version); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	return &version, nil
}

// FormatDownloads formats download count for display
func FormatDownloads(count int) string {
	switch {
	case count >= 1_000_000:
		return fmt.Sprintf("%.1fM", float64(count)/1_000_000)
	case count >= 1_000:
		return fmt.Sprintf("%.1fK", float64(count)/1_000)
	default:
		return fmt.Sprintf("%d", count)
	}
}

// JoinLoaders formats loader list for display
func JoinLoaders(loaders []string) string {
	return strings.Join(loaders, ", ")
}
