package fabric

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
)

// readGolden loads a fixture from testdata/. Go auto-ignores testdata/ in builds.
func readGolden(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("testdata", name))
	if err != nil {
		t.Fatalf("read fixture %s: %v", name, err)
	}
	return data
}

// These goldens are pure fixture decoding / MergeProfile and never touch the
// process-global metaBase/metaHTTP, so they are safe; we deliberately omit
// t.Parallel() to keep all fabric tests uniformly non-parallel (metaBase is
// process-global and live tests mutate nothing, but defaulting to no-parallel
// here avoids any future footgun if a seam is added).

func TestGolden_FabricLoaderList(t *testing.T) {
	var entries []loaderMetaEntry
	if err := json.Unmarshal(readGolden(t, "fabric_loader_list.json"), &entries); err != nil {
		t.Fatalf("decode []loaderMetaEntry: %v", err)
	}
	if len(entries) == 0 {
		t.Fatal("no loader entries")
	}

	haveVersion := false
	haveStable := false
	for _, e := range entries {
		if e.Loader.Version != "" {
			haveVersion = true
		}
		if e.Loader.Stable && e.Loader.Version != "" {
			haveStable = true
		}
	}
	if !haveVersion {
		t.Error("no entry with non-empty Loader.Version")
	}
	if !haveStable {
		t.Error("no entry with Stable==true (pickStableLoaderVersion would have nothing to return)")
	}
}

func TestGolden_FabricProfileMerge(t *testing.T) {
	profileJSON := readGolden(t, "fabric_profile.json")

	// Minimal parent: MergeProfile only requires a non-nil parent with an ID
	// matching the profile's inheritsFrom (1.21 here).
	parent := &core.VersionDetails{ID: "1.21"}

	merged, err := MergeProfile(parent, profileJSON)
	if err != nil {
		t.Fatalf("MergeProfile: %v", err)
	}
	if merged.MainClass == "" {
		t.Error("merged.MainClass empty")
	}
	if len(merged.Libraries) == 0 {
		t.Error("merged.Libraries empty")
	}
}
