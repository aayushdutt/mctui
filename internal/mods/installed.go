package mods

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/quasar/mctui/internal/core"
)

// InstalledJar is a mod file present under the instance mods directory.
type InstalledJar struct {
	Name string // file name only
	Path string // absolute path
	Size int64
}

// ListInstalledJars returns .jar files directly in ModsDir (not subfolders), sorted by name.
func ListInstalledJars(inst *core.Instance) ([]InstalledJar, error) {
	if inst == nil {
		return nil, fmt.Errorf("instance required")
	}
	dir := ModsDir(inst)
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("mods folder: %w", err)
	}
	var out []InstalledJar
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := e.Name()
		if !strings.EqualFold(filepath.Ext(name), ".jar") {
			continue
		}
		abs := filepath.Join(dir, name)
		fi, err := e.Info()
		if err != nil {
			continue
		}
		out = append(out, InstalledJar{Name: name, Path: abs, Size: fi.Size()})
	}
	sort.Slice(out, func(i, j int) bool { return strings.ToLower(out[i].Name) < strings.ToLower(out[j].Name) })
	return out, nil
}

// EnsureModsDir creates the mods directory if missing.
func EnsureModsDir(inst *core.Instance) error {
	if inst == nil {
		return fmt.Errorf("instance required")
	}
	dir := ModsDir(inst)
	return os.MkdirAll(dir, 0755)
}

// RemoveInstalledJar deletes a jar from the instance mods folder (top-level only).
func RemoveInstalledJar(inst *core.Instance, baseName string) error {
	if inst == nil {
		return fmt.Errorf("instance required")
	}
	baseName = filepath.Base(baseName)
	if !strings.EqualFold(filepath.Ext(baseName), ".jar") {
		return fmt.Errorf("not a .jar file")
	}
	path := filepath.Join(ModsDir(inst), baseName)
	if st, err := os.Stat(path); err != nil || st.IsDir() {
		return fmt.Errorf("mod file not found")
	}
	return os.Remove(path)
}
