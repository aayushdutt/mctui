// Package core contains business logic independent of the UI.
// This is the heart of the application - all game-related logic lives here.
package core

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/aayushdutt/mctui/internal/fsutil"
)

// Instance represents a Minecraft instance
type Instance struct {
	ID         string    `json:"id"`
	Name       string    `json:"name"`
	Version    string    `json:"version"`   // Minecraft version (e.g., "1.21.4")
	Loader     string    `json:"loader"`    // Loader type: vanilla, fabric, forge, quilt
	LoaderVer  string    `json:"loaderVer"` // Loader version
	Path       string    `json:"path"`      // Path to instance directory
	JavaPath   string    `json:"javaPath"`  // Path to Java executable (optional)
	JVMArgs    []string  `json:"jvmArgs"`   // Additional JVM arguments
	LastPlayed time.Time `json:"lastPlayed"`
	// CreatedAt is set when the instance is first saved (wizard / Create). Used for list order when LastPlayed is zero.
	CreatedAt time.Time `json:"createdAt,omitempty"`
	PlayTime  int64     `json:"playTime"` // Total playtime in seconds

	// Caching fields for offline support
	IsFullyDownloaded bool      `json:"isFullyDownloaded"` // All files downloaded and ready
	CachedAt          time.Time `json:"cachedAt"`          // When instance was last fully cached
	// DownloadCacheKey matches LaunchDownloadKey when isFullyDownloaded was set; used to invalidate after loader/MC changes.
	DownloadCacheKey string `json:"downloadCacheKey,omitempty"`

	// InstallStarterFabricMods is true when the user opted into the default Fabric bundle at instance creation.
	// Cleared after that bundle is installed successfully at launch (see mods package).
	InstallStarterFabricMods bool `json:"installStarterFabricMods,omitempty"`
}

// SanitizeInstanceDirName turns a display name into a filesystem-safe folder base name.
// It mirrors how launchers like PrismLauncher derive an instance folder from its name:
// path-invalid characters become '-', control characters are dropped, and trailing dots
// or spaces (invalid on Windows) are trimmed. Spaces, case, and unicode are preserved so
// the folder stays recognizable. May return "" (e.g. a name that is all dots); callers
// should fall back to a default in that case.
func SanitizeInstanceDirName(name string) string {
	var b strings.Builder
	for _, r := range name {
		switch {
		case r < 0x20:
			// drop control characters
		case strings.ContainsRune(`<>:"/\|?*`, r):
			b.WriteRune('-')
		default:
			b.WriteRune(r)
		}
	}
	out := strings.TrimSpace(b.String())
	out = strings.TrimRight(out, ". ")
	return strings.TrimSpace(out)
}

// LaunchDownloadKey fingerprints game version, loader, and loader version for download skip logic.
// Empty LoaderVer is normalized (e.g. vanilla); empty Loader is treated as vanilla.
func LaunchDownloadKey(inst *Instance) string {
	if inst == nil {
		return ""
	}
	loader := strings.ToLower(strings.TrimSpace(inst.Loader))
	if loader == "" {
		loader = "vanilla"
	}
	return inst.Version + "|" + loader + "|" + strings.TrimSpace(inst.LoaderVer)
}

// RecencyForSort picks a timestamp for ordering instances (most recent first).
// It is the later of LastPlayed and CreatedAt (each ignored if zero). Usually LastPlayed ≥ CreatedAt.
func RecencyForSort(inst *Instance) time.Time {
	if inst == nil {
		return time.Time{}
	}
	lp, ca := inst.LastPlayed, inst.CreatedAt
	switch {
	case lp.IsZero() && ca.IsZero():
		return time.Time{}
	case lp.IsZero():
		return ca
	case ca.IsZero():
		return lp
	default:
		if lp.After(ca) {
			return lp
		}
		return ca
	}
}

// InstanceManager handles instance CRUD operations. It is safe for concurrent
// use: the TUI event loop, Cmd goroutines (Load/List), and a running launch
// (Update/UpdateLastPlayed/AddPlayTime) all go through it.
type InstanceManager struct {
	mu        sync.Mutex
	basePath  string
	instances map[string]*Instance
	// onDisk is the instance.json content last read or written per ID, so a
	// reload can tell a hand edit (take it) from our own save (keep memory).
	onDisk map[string][]byte
}

// NewInstanceManager creates a new instance manager
func NewInstanceManager(basePath string) *InstanceManager {
	return &InstanceManager{
		basePath:  basePath,
		instances: make(map[string]*Instance),
		onDisk:    make(map[string][]byte),
	}
}

// Load syncs the in-memory set with the instances on disk.
//
// Instances already loaded keep their *Instance: a running launch holds that
// pointer and persists through the manager, so swapping in a fresh copy would
// let the stale one overwrite newer state (e.g. playtime credited at game exit).
// If an instance.json changed on disk since mctui last read or wrote it (a hand
// edit — the only way to change version/loader), its contents are taken into
// the existing pointer. New folders are added; instances whose folder is gone
// are dropped.
//
// The whole scan runs under the lock so a concurrent save or delete can't
// interleave with it (and on Windows, a save's rename can't race a read here).
func (im *InstanceManager) Load() error {
	im.mu.Lock()
	defer im.mu.Unlock()

	instancesPath := filepath.Join(im.basePath, "instances")
	entries, err := os.ReadDir(instancesPath)
	if err != nil && !os.IsNotExist(err) {
		return err
	}

	next := make(map[string]*Instance, len(entries))
	onDisk := make(map[string][]byte, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}

		configPath := filepath.Join(instancesPath, entry.Name(), "instance.json")
		data, err := os.ReadFile(configPath)
		if err != nil {
			continue // Skip instances without config
		}

		var inst Instance
		if err := json.Unmarshal(data, &inst); err != nil {
			continue // Skip malformed configs
		}

		if cur, ok := im.instances[inst.ID]; ok {
			if !bytes.Equal(data, im.onDisk[inst.ID]) {
				*cur = inst // edited outside mctui: take the file's contents
			}
			next[inst.ID] = cur
		} else {
			next[inst.ID] = &inst
		}
		onDisk[inst.ID] = data
	}

	// Keep known instances the scan couldn't read while their folder still
	// exists (e.g. an instance.json being hand-edited into invalid JSON).
	for id, cur := range im.instances {
		if _, ok := next[id]; ok {
			continue
		}
		if _, err := os.Stat(cur.Path); err == nil {
			next[id] = cur
			onDisk[id] = im.onDisk[id]
		}
	}
	im.instances = next
	im.onDisk = onDisk
	return nil
}

// List returns all instances
func (im *InstanceManager) List() []*Instance {
	im.mu.Lock()
	defer im.mu.Unlock()
	result := make([]*Instance, 0, len(im.instances))
	for _, inst := range im.instances {
		result = append(result, inst)
	}
	return result
}

// Get returns an instance by ID
func (im *InstanceManager) Get(id string) (*Instance, bool) {
	im.mu.Lock()
	defer im.mu.Unlock()
	inst, ok := im.instances[id]
	return inst, ok
}

// idTaken reports whether an instance ID (folder basename) is already in use,
// either in memory or on disk. Caller holds im.mu.
func (im *InstanceManager) idTaken(id string) bool {
	if _, ok := im.instances[id]; ok {
		return true
	}
	_, err := os.Stat(filepath.Join(im.basePath, "instances", id))
	return err == nil
}

// generateInstanceID derives a unique, filesystem-safe folder name from a display
// name. Duplicates get a " (2)", " (3)", … suffix (PrismLauncher-style), so display
// names may collide freely while folder names stay unique and recognizable.
// Caller holds im.mu.
func (im *InstanceManager) generateInstanceID(name string) string {
	base := SanitizeInstanceDirName(name)
	if base == "" {
		base = "instance"
	}
	id := base
	for n := 2; im.idTaken(id); n++ {
		id = fmt.Sprintf("%s (%d)", base, n)
	}
	return id
}

// Create creates a new instance. If inst.ID is empty it is derived from inst.Name
// (sanitized + de-duplicated); the display Name is kept verbatim and stays editable
// independently of the folder/ID.
func (im *InstanceManager) Create(inst *Instance) error {
	im.mu.Lock()
	defer im.mu.Unlock()
	if inst.ID == "" {
		inst.ID = im.generateInstanceID(inst.Name)
	}
	instPath := filepath.Join(im.basePath, "instances", inst.ID)

	// Create instance directory
	if err := os.MkdirAll(instPath, 0755); err != nil {
		return err
	}

	inst.Path = instPath
	if inst.CreatedAt.IsZero() {
		inst.CreatedAt = time.Now()
	}

	// Save instance config
	if err := im.save(inst); err != nil {
		return err
	}

	im.instances[inst.ID] = inst
	return nil
}

// Delete removes an instance
func (im *InstanceManager) Delete(id string) error {
	im.mu.Lock()
	defer im.mu.Unlock()
	inst, ok := im.instances[id]
	if !ok {
		return nil
	}

	// Remove from disk
	if err := os.RemoveAll(inst.Path); err != nil {
		return err
	}

	delete(im.instances, id)
	delete(im.onDisk, id)
	return nil
}

// save writes instance config to disk atomically. Caller holds im.mu, which
// also serializes concurrent saves of the same instance.json.
func (im *InstanceManager) save(inst *Instance) error {
	data, err := json.MarshalIndent(inst, "", "  ")
	if err != nil {
		return err
	}

	configPath := filepath.Join(inst.Path, "instance.json")
	if err := fsutil.WriteFileAtomic(configPath, data, 0o644); err != nil {
		return err
	}
	im.onDisk[inst.ID] = data
	return nil
}

// Update updates an existing instance
func (im *InstanceManager) Update(inst *Instance) error {
	im.mu.Lock()
	defer im.mu.Unlock()
	im.instances[inst.ID] = inst
	return im.save(inst)
}

// UpdateLastPlayed updates the last played timestamp
func (im *InstanceManager) UpdateLastPlayed(id string) error {
	im.mu.Lock()
	defer im.mu.Unlock()
	inst, ok := im.instances[id]
	if !ok {
		return nil
	}
	inst.LastPlayed = time.Now()
	return im.save(inst)
}

// AddPlayTime credits seconds to an instance's total playtime and persists it.
// Called when a launched game exits with the session's wall-clock duration. A
// non-positive delta (or unknown id) is a no-op so a crash-on-launch never
// corrupts the total.
func (im *InstanceManager) AddPlayTime(id string, seconds int64) error {
	if seconds <= 0 {
		return nil
	}
	im.mu.Lock()
	defer im.mu.Unlock()
	inst, ok := im.instances[id]
	if !ok {
		return nil
	}
	inst.PlayTime += seconds
	return im.save(inst)
}
