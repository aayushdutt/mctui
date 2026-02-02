// Package core version handling.
// Manages Minecraft version manifests and version information.
package core

import "time"

// VersionType represents the type of Minecraft version
type VersionType string

const (
	VersionTypeRelease  VersionType = "release"
	VersionTypeSnapshot VersionType = "snapshot"
	VersionTypeOldBeta  VersionType = "old_beta"
	VersionTypeOldAlpha VersionType = "old_alpha"
)

// LoaderType represents the mod loader type
type LoaderType string

const (
	LoaderVanilla  LoaderType = "vanilla"
	LoaderFabric   LoaderType = "fabric"
	LoaderForge    LoaderType = "forge"
	LoaderQuilt    LoaderType = "quilt"
	LoaderNeoForge LoaderType = "neoforge"
)

// Version represents a Minecraft version from the manifest
type Version struct {
	ID          string      `json:"id"`
	Type        VersionType `json:"type"`
	URL         string      `json:"url"`
	ReleaseTime time.Time   `json:"releaseTime"`
	SHA1        string      `json:"sha1"`
}

// VersionManifest is the root of Mojang's version manifest
type VersionManifest struct {
	Latest   LatestVersions `json:"latest"`
	Versions []Version      `json:"versions"`
}

// LatestVersions contains the latest release and snapshot
type LatestVersions struct {
	Release  string `json:"release"`
	Snapshot string `json:"snapshot"`
}

// LoaderVersion represents a mod loader version
type LoaderVersion struct {
	Version   string `json:"version"`
	Stable    bool   `json:"stable"`
	MCVersion string `json:"mcVersion"` // Compatible MC version
}

// VersionDetails contains full version metadata (from version JSON)
type VersionDetails struct {
	ID                 string         `json:"id"`
	Type               VersionType    `json:"type"`
	MainClass          string         `json:"mainClass"`
	MinecraftArguments string         `json:"minecraftArguments,omitempty"`
	Arguments          *Arguments     `json:"arguments,omitempty"`
	Libraries          []Library      `json:"libraries"`
	AssetIndex         AssetIndexRef  `json:"assetIndex"`
	Assets             string         `json:"assets"`
	Downloads          Downloads      `json:"downloads"`
	JavaVersion        JavaVersionReq `json:"javaVersion"`
	ReleaseTime        time.Time      `json:"releaseTime"`
	Time               time.Time      `json:"time"`
}

// Arguments contains game and JVM arguments (modern format)
type Arguments struct {
	Game []interface{} `json:"game"`
	JVM  []interface{} `json:"jvm"`
}

// Library represents a dependency library
type Library struct {
	Name      string            `json:"name"`
	Downloads *LibraryDownloads `json:"downloads,omitempty"`
	Rules     []Rule            `json:"rules,omitempty"`
	Natives   map[string]string `json:"natives,omitempty"`
}

// LibraryDownloads contains artifact download info
type LibraryDownloads struct {
	Artifact    *Artifact            `json:"artifact,omitempty"`
	Classifiers map[string]*Artifact `json:"classifiers,omitempty"`
}

// Artifact represents a downloadable file
type Artifact struct {
	Path string `json:"path"`
	SHA1 string `json:"sha1"`
	Size int64  `json:"size"`
	URL  string `json:"url"`
}

// Rule represents OS/feature-based conditions
type Rule struct {
	Action   string    `json:"action"` // allow or disallow
	OS       *OSRule   `json:"os,omitempty"`
	Features *Features `json:"features,omitempty"`
}

// OSRule specifies OS conditions
type OSRule struct {
	Name    string `json:"name,omitempty"`
	Version string `json:"version,omitempty"`
	Arch    string `json:"arch,omitempty"`
}

// Features specifies feature flags
type Features struct {
	IsDemoUser        bool `json:"is_demo_user,omitempty"`
	HasCustomRes      bool `json:"has_custom_resolution,omitempty"`
	HasQuickPlaysup   bool `json:"has_quick_plays_support,omitempty"`
	IsQuickPlaySingle bool `json:"is_quick_play_singleplayer,omitempty"`
	IsQuickPlayMulti  bool `json:"is_quick_play_multiplayer,omitempty"`
	IsQuickPlayRealms bool `json:"is_quick_play_realms,omitempty"`
}

// AssetIndexRef references the asset index
type AssetIndexRef struct {
	ID        string `json:"id"`
	SHA1      string `json:"sha1"`
	Size      int64  `json:"size"`
	TotalSize int64  `json:"totalSize"`
	URL       string `json:"url"`
}

// Downloads contains client/server download info
type Downloads struct {
	Client         *Artifact `json:"client,omitempty"`
	ClientMappings *Artifact `json:"client_mappings,omitempty"`
	Server         *Artifact `json:"server,omitempty"`
	ServerMappings *Artifact `json:"server_mappings,omitempty"`
}

// JavaVersionReq specifies required Java version
type JavaVersionReq struct {
	Component    string `json:"component"`
	MajorVersion int    `json:"majorVersion"`
}
