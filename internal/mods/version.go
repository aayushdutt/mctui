package mods

import (
	"strings"

	"github.com/aayushdutt/mctui/internal/api"
)

// PickBestFabricVersion chooses a version to install from Modrinth's filtered list.
// The API returns versions newest-first; we prefer an explicit "release" when present.
func PickBestFabricVersion(versions []api.ProjectVersion) *api.ProjectVersion {
	if len(versions) == 0 {
		return nil
	}
	for i := range versions {
		if versions[i].VersionType == "release" {
			return &versions[i]
		}
	}
	return &versions[0]
}

// PrimaryJar returns the main .jar file for a project version (primary flag, else first .jar).
func PrimaryJar(v *api.ProjectVersion) *api.VersionFile {
	if v == nil {
		return nil
	}
	for i := range v.Files {
		f := &v.Files[i]
		if f.Primary && strings.HasSuffix(strings.ToLower(f.Filename), ".jar") {
			return f
		}
	}
	for i := range v.Files {
		f := &v.Files[i]
		if strings.HasSuffix(strings.ToLower(f.Filename), ".jar") {
			return f
		}
	}
	return nil
}
