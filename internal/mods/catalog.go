package mods

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/mctui/mctui/internal/core"
)

const catalogFileName = ".mctui-modrinth.json"

// ModrinthCatalog tracks Modrinth projects installed via mctui (for browse badges).
type ModrinthCatalog struct {
	Projects []ModrinthCatalogEntry `json:"projects"`
}

// ModrinthCatalogEntry records one installed project ↔ jar file.
type ModrinthCatalogEntry struct {
	ProjectID string `json:"projectId"`
	Slug      string `json:"slug"`
	File      string `json:"file"` // basename in mods dir
}

func catalogPath(inst *core.Instance) string {
	if inst == nil {
		return ""
	}
	return filepath.Join(ModsDir(inst), catalogFileName)
}

// LoadModrinthCatalog reads the catalog, or returns empty if missing.
func LoadModrinthCatalog(inst *core.Instance) (*ModrinthCatalog, error) {
	if inst == nil {
		return &ModrinthCatalog{}, nil
	}
	p := catalogPath(inst)
	data, err := os.ReadFile(p)
	if err != nil {
		if os.IsNotExist(err) {
			return &ModrinthCatalog{}, nil
		}
		return nil, err
	}
	var c ModrinthCatalog
	if err := json.Unmarshal(data, &c); err != nil {
		return nil, fmt.Errorf("catalog: %w", err)
	}
	return &c, nil
}

// SaveModrinthCatalog writes the catalog atomically.
func SaveModrinthCatalog(inst *core.Instance, c *ModrinthCatalog) error {
	if inst == nil || c == nil {
		return fmt.Errorf("instance and catalog required")
	}
	if err := os.MkdirAll(ModsDir(inst), 0755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	p := catalogPath(inst)
	tmp := p + ".tmp"
	if err := os.WriteFile(tmp, data, 0644); err != nil {
		return err
	}
	return os.Rename(tmp, p)
}

// RecordModrinthInstall appends or updates an entry and saves.
func RecordModrinthInstall(inst *core.Instance, projectID, slug, fileBase string) error {
	c, err := LoadModrinthCatalog(inst)
	if err != nil {
		return fmt.Errorf("read catalog: %w", err)
	}
	fileBase = filepath.Base(fileBase)
	var out []ModrinthCatalogEntry
	for _, e := range c.Projects {
		if e.ProjectID != projectID {
			out = append(out, e)
		}
	}
	out = append(out, ModrinthCatalogEntry{
		ProjectID: projectID,
		Slug:      slug,
		File:      fileBase,
	})
	c.Projects = out
	return SaveModrinthCatalog(inst, c)
}

// DropCatalogEntriesForJar removes catalog rows that reference a deleted jar file (basename match).
func DropCatalogEntriesForJar(inst *core.Instance, fileBase string) error {
	if inst == nil {
		return fmt.Errorf("instance required")
	}
	fileBase = filepath.Base(fileBase)
	c, err := LoadModrinthCatalog(inst)
	if err != nil {
		return err
	}
	var out []ModrinthCatalogEntry
	for _, e := range c.Projects {
		if strings.EqualFold(e.File, fileBase) {
			continue
		}
		out = append(out, e)
	}
	if len(out) == len(c.Projects) {
		return nil
	}
	c.Projects = out
	return SaveModrinthCatalog(inst, c)
}

// ProjectRecorded returns true if projectID is in catalog and the jar still exists on disk.
func ProjectRecorded(c *ModrinthCatalog, jars []InstalledJar, projectID string) bool {
	if c == nil {
		return false
	}
	var wantFile string
	for _, p := range c.Projects {
		if p.ProjectID == projectID {
			wantFile = p.File
			break
		}
	}
	if wantFile == "" {
		return false
	}
	for _, j := range jars {
		if strings.EqualFold(j.Name, wantFile) {
			return true
		}
	}
	return false
}

// SlugHeuristicMatch returns true if slug appears to match a jar name (manual installs).
func SlugHeuristicMatch(jarNames []string, slug string) bool {
	if slug == "" {
		return false
	}
	slug = strings.ToLower(slug)
	slim := strings.ReplaceAll(slug, "-", "")
	for _, n := range jarNames {
		low := strings.ToLower(n)
		if strings.Contains(low, slug) || strings.Contains(strings.ReplaceAll(low, "-", ""), slim) {
			return true
		}
	}
	return false
}

// IsModrinthProjectInstalled combines catalog + on-disk file + slug heuristic.
func IsModrinthProjectInstalled(c *ModrinthCatalog, jars []InstalledJar, projectID, slug string) bool {
	if ProjectRecorded(c, jars, projectID) {
		return true
	}
	names := make([]string, len(jars))
	for i := range jars {
		names[i] = jars[i].Name
	}
	return SlugHeuristicMatch(names, slug)
}
