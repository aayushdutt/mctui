// Package core contains business logic independent of the UI.
// This is the heart of the application - all game-related logic lives here.
package core

import (
	"encoding/json"
	"os"
	"path/filepath"
	"time"
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
	PlayTime   int64     `json:"playTime"` // Total playtime in seconds

	// Caching fields for offline support
	IsFullyDownloaded bool      `json:"isFullyDownloaded"` // All files downloaded and ready
	CachedAt          time.Time `json:"cachedAt"`          // When instance was last fully cached
}

// InstanceManager handles instance CRUD operations
type InstanceManager struct {
	basePath  string
	instances map[string]*Instance
}

// NewInstanceManager creates a new instance manager
func NewInstanceManager(basePath string) *InstanceManager {
	return &InstanceManager{
		basePath:  basePath,
		instances: make(map[string]*Instance),
	}
}

// Load reads all instances from disk
func (im *InstanceManager) Load() error {
	instancesPath := filepath.Join(im.basePath, "instances")

	entries, err := os.ReadDir(instancesPath)
	if os.IsNotExist(err) {
		// No instances directory yet, that's fine
		return nil
	}
	if err != nil {
		return err
	}

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

		im.instances[inst.ID] = &inst
	}

	return nil
}

// List returns all instances
func (im *InstanceManager) List() []*Instance {
	result := make([]*Instance, 0, len(im.instances))
	for _, inst := range im.instances {
		result = append(result, inst)
	}
	return result
}

// Get returns an instance by ID
func (im *InstanceManager) Get(id string) (*Instance, bool) {
	inst, ok := im.instances[id]
	return inst, ok
}

// Create creates a new instance
func (im *InstanceManager) Create(inst *Instance) error {
	instPath := filepath.Join(im.basePath, "instances", inst.ID)

	// Create instance directory
	if err := os.MkdirAll(instPath, 0755); err != nil {
		return err
	}

	inst.Path = instPath

	// Save instance config
	if err := im.save(inst); err != nil {
		return err
	}

	im.instances[inst.ID] = inst
	return nil
}

// Delete removes an instance
func (im *InstanceManager) Delete(id string) error {
	inst, ok := im.instances[id]
	if !ok {
		return nil
	}

	// Remove from disk
	if err := os.RemoveAll(inst.Path); err != nil {
		return err
	}

	delete(im.instances, id)
	return nil
}

// save writes instance config to disk
func (im *InstanceManager) save(inst *Instance) error {
	data, err := json.MarshalIndent(inst, "", "  ")
	if err != nil {
		return err
	}

	configPath := filepath.Join(inst.Path, "instance.json")
	return os.WriteFile(configPath, data, 0644)
}

// Update updates an existing instance
func (im *InstanceManager) Update(inst *Instance) error {
	im.instances[inst.ID] = inst
	return im.save(inst)
}

// UpdateLastPlayed updates the last played timestamp
func (im *InstanceManager) UpdateLastPlayed(id string) error {
	inst, ok := im.instances[id]
	if !ok {
		return nil
	}
	inst.LastPlayed = time.Now()
	return im.save(inst)
}
