package fabric

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/Masterminds/semver/v3"
	"github.com/aayushdutt/mctui/internal/core"
)

type fabricProfile struct {
	ID           string             `json:"id"`
	InheritsFrom string             `json:"inheritsFrom"`
	MainClass    string             `json:"mainClass"`
	Arguments    *core.Arguments    `json:"arguments,omitempty"`
	Libraries    []fabricLibraryRaw `json:"libraries"`
}

type fabricLibraryRaw struct {
	Name  string      `json:"name"`
	URL   string      `json:"url,omitempty"`
	SHA1  string      `json:"sha1,omitempty"`
	Size  int64       `json:"size,omitempty"`
	Rules []core.Rule `json:"rules,omitempty"`
}

const defaultMavenBase = "https://repo1.maven.org/maven2/"

// mergeProfileCacheSchema: bump when MergeProfile output is incompatible with older cached JSON.
const mergeProfileCacheSchema = 1

func mergedProfileCacheFile(cacheDir, gameVer, loaderVer string) string {
	return filepath.Join(cacheDir, fmt.Sprintf("fabric-merged-schema%d-%s-%s.json", mergeProfileCacheSchema, gameVer, loaderVer))
}

// MergeProfile merges Fabric profile JSON into Mojang parent VersionDetails (ID unchanged = parent.ID).
func MergeProfile(parent *core.VersionDetails, profileJSON []byte) (*core.VersionDetails, error) {
	if parent == nil {
		return nil, fmt.Errorf("parent version details required")
	}
	var prof fabricProfile
	if err := json.Unmarshal(profileJSON, &prof); err != nil {
		return nil, fmt.Errorf("decode fabric profile: %w", err)
	}
	if prof.InheritsFrom != "" && prof.InheritsFrom != parent.ID {
		return nil, fmt.Errorf("fabric profile inheritsFrom %q does not match game version %q", prof.InheritsFrom, parent.ID)
	}

	childLibs, err := normalizeFabricLibraries(prof.Libraries)
	if err != nil {
		return nil, err
	}

	out := *parent
	mergedLibs := mergeLibrariesByMavenIdentity(parent.Libraries, childLibs)
	mergedLibs = dedupeLibrariesByArtifactPath(mergedLibs)
	out.Libraries = dropOW2AsmAllWhenModularPresent(mergedLibs)
	if prof.MainClass != "" {
		out.MainClass = prof.MainClass
	}
	out.Arguments = mergeArguments(parent.Arguments, prof.Arguments)
	return &out, nil
}

// dropOW2AsmAllWhenModularPresent: asm-all vs modular asm are different coordinates but duplicate
// org.objectweb.asm on the classpath; drop the fat jar when modular jars are present.
func dropOW2AsmAllWhenModularPresent(libs []core.Library) []core.Library {
	const ow2Group = "org.ow2.asm"
	modular := false
	for _, lib := range libs {
		g, a, _, _, ok := parseMavenLibraryParts(lib.Name)
		if ok && g == ow2Group && a != "asm-all" {
			modular = true
			break
		}
	}
	if !modular {
		return libs
	}
	out := make([]core.Library, 0, len(libs))
	for _, lib := range libs {
		g, a, _, _, ok := parseMavenLibraryParts(lib.Name)
		if ok && g == ow2Group && a == "asm-all" {
			continue
		}
		out = append(out, lib)
	}
	return out
}

func mavenIdentityKey(name string) (key, version string, ok bool) {
	g, a, v, c, ok := parseMavenLibraryParts(name)
	if !ok {
		return "", "", false
	}
	return g + ":" + a + ":" + c, v, true
}

func mergeLibrariesByMavenIdentity(parent, child []core.Library) []core.Library {
	var out []core.Library
	at := make(map[string]int)
	apply := func(lib core.Library) {
		k, ver, ok := mavenIdentityKey(lib.Name)
		if !ok {
			out = append(out, lib)
			return
		}
		i, exists := at[k]
		if !exists {
			at[k] = len(out)
			out = append(out, lib)
			return
		}
		_, ev, _ := mavenIdentityKey(out[i].Name)
		if compareMavenVersions(ver, ev) > 0 {
			out[i] = lib
		}
	}
	for _, lib := range parent {
		apply(lib)
	}
	for _, lib := range child {
		apply(lib)
	}
	return out
}

func compareMavenVersions(a, b string) int {
	if a == b {
		return 0
	}
	va, errA := semver.NewVersion(a)
	vb, errB := semver.NewVersion(b)
	if errA == nil && errB == nil {
		return va.Compare(vb)
	}
	return strings.Compare(a, b)
}

func dedupeLibrariesByArtifactPath(libs []core.Library) []core.Library {
	seen := make(map[string]struct{})
	out := make([]core.Library, 0, len(libs))
	for _, lib := range libs {
		if lib.Downloads == nil || lib.Downloads.Artifact == nil {
			out = append(out, lib)
			continue
		}
		p := lib.Downloads.Artifact.Path
		if p == "" {
			out = append(out, lib)
			continue
		}
		if _, dup := seen[p]; dup {
			continue
		}
		seen[p] = struct{}{}
		out = append(out, lib)
	}
	return out
}

func normalizeFabricLibraries(raw []fabricLibraryRaw) ([]core.Library, error) {
	out := make([]core.Library, 0, len(raw))
	for _, lib := range raw {
		base := strings.TrimSpace(lib.URL)
		if base == "" {
			base = defaultMavenBase
		}
		artifactPath, err := mavenArtifactPath(lib.Name)
		if err != nil {
			return nil, err
		}
		jarURL := joinRepoURL(base, artifactPath)
		coreLib := core.Library{
			Name:  lib.Name,
			Rules: lib.Rules,
			Downloads: &core.LibraryDownloads{
				Artifact: &core.Artifact{
					Path: artifactPath,
					URL:  jarURL,
					SHA1: lib.SHA1,
					Size: lib.Size,
				},
			},
		}
		out = append(out, coreLib)
	}
	return out, nil
}

func mergeArguments(parent, child *core.Arguments) *core.Arguments {
	if child == nil || len(child.JVM) == 0 {
		if parent == nil {
			return nil
		}
		cp := *parent
		return &cp
	}
	mergedJVM := append([]interface{}{}, child.JVM...)
	if parent != nil && len(parent.JVM) > 0 {
		mergedJVM = append(mergedJVM, parent.JVM...)
	}
	out := &core.Arguments{JVM: mergedJVM}
	if parent != nil && len(parent.Game) > 0 {
		out.Game = append(out.Game, parent.Game...)
	}
	if len(child.Game) > 0 {
		out.Game = append(out.Game, child.Game...)
	}
	return out
}
