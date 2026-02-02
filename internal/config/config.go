// Package config handles application configuration and paths.
package config

import (
	"encoding/json"
	"os"
	"path/filepath"
)

// Config holds the application configuration
type Config struct {
	// Paths
	DataDir      string `json:"dataDir"`
	InstancesDir string `json:"instancesDir"`
	AssetsDir    string `json:"assetsDir"`
	LibrariesDir string `json:"librariesDir"`

	// Java
	JavaPath string   `json:"javaPath"`
	JVMArgs  []string `json:"jvmArgs"`

	// UI preferences
	Theme         string `json:"theme"`
	ShowSnapshots bool   `json:"showSnapshots"`

	// Auth
	MSAClientID string `json:"msaClientID"`
}

const (
	DefaultMSAClientID = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb"
)

// DefaultConfig returns a config with sensible defaults
func DefaultConfig() *Config {
	dataDir := getDefaultDataDir()
	return &Config{
		DataDir:       dataDir,
		InstancesDir:  filepath.Join(dataDir, "instances"),
		AssetsDir:     filepath.Join(dataDir, "assets"),
		LibrariesDir:  filepath.Join(dataDir, "libraries"),
		JVMArgs:       []string{"-Xmx2G", "-Xms512M"},
		Theme:         "dark",
		ShowSnapshots: false,
		MSAClientID:   DefaultMSAClientID,
	}
}

// Load reads config from disk
func Load() (*Config, error) {
	cfg := DefaultConfig()

	configPath := filepath.Join(cfg.DataDir, "config.json")
	data, err := os.ReadFile(configPath)
	if os.IsNotExist(err) {
		return cfg, nil
	}
	if err != nil {
		return nil, err
	}

	if err := json.Unmarshal(data, cfg); err != nil {
		return nil, err
	}

	// Fallback to default ID if config file had empty string or missing field
	if cfg.MSAClientID == "" {
		cfg.MSAClientID = DefaultMSAClientID
	}

	return cfg, nil
}

// Save writes config to disk
func (c *Config) Save() error {
	if err := os.MkdirAll(c.DataDir, 0755); err != nil {
		return err
	}

	data, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}

	configPath := filepath.Join(c.DataDir, "config.json")
	return os.WriteFile(configPath, data, 0644)
}

// EnsureDirs creates all required directories
func (c *Config) EnsureDirs() error {
	dirs := []string{c.DataDir, c.InstancesDir, c.AssetsDir, c.LibrariesDir}
	for _, dir := range dirs {
		if err := os.MkdirAll(dir, 0755); err != nil {
			return err
		}
	}
	return nil
}

func getDefaultDataDir() string {
	// Check for portable mode first
	exe, _ := os.Executable()
	portablePath := filepath.Join(filepath.Dir(exe), "data")
	if _, err := os.Stat(portablePath); err == nil {
		return portablePath
	}

	// Use XDG/platform-specific directories
	if xdg := os.Getenv("XDG_DATA_HOME"); xdg != "" {
		return filepath.Join(xdg, "mctui")
	}

	home, _ := os.UserHomeDir()
	switch {
	case os.Getenv("APPDATA") != "": // Windows
		return filepath.Join(os.Getenv("APPDATA"), "mctui")
	default: // Linux/macOS
		return filepath.Join(home, ".local", "share", "mctui")
	}
}
