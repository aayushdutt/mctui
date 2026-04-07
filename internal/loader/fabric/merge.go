package fabric

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/quasar/mctui/internal/core"
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

// MergeProfile merges a Fabric launcher profile JSON into Mojang parent VersionDetails.
// The returned VersionDetails.ID is always parent.ID so the vanilla client jar path stays correct.
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
	out.Libraries = append(append([]core.Library{}, parent.Libraries...), childLibs...)
	if prof.MainClass != "" {
		out.MainClass = prof.MainClass
	}
	out.Arguments = mergeArguments(parent.Arguments, prof.Arguments)
	return &out, nil
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
	if child != nil && len(child.Game) > 0 {
		out.Game = append(out.Game, child.Game...)
	}
	return out
}
