package fabric

import (
	"context"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

// TestResolveVersion_OfflineUsesCache verifies that an offline (offline-account)
// launch of a Fabric instance with a saved loader version and a cached merged
// profile resolves from the cache without touching the network.
func TestResolveVersion_OfflineUsesCache(t *testing.T) {
	dir := t.TempDir()
	mojang := api.NewMojangClient(dir)
	inst := &core.Instance{Version: "1.21.6", Loader: "fabric", LoaderVer: "0.16.0"}

	cacheFile := mergedProfileCacheFile(mojang.VersionCacheDir(), inst.Version, inst.LoaderVer)
	want := &core.VersionDetails{ID: "1.21.6", MainClass: "net.fabricmc.loader.impl.launch.knot.KnotClient"}
	if err := saveMergedCache(cacheFile, want); err != nil {
		t.Fatalf("seed cache: %v", err)
	}

	got, err := ResolveVersion(context.Background(), mojang, inst, true)
	if err != nil {
		t.Fatalf("offline resolve with cache should succeed without network: %v", err)
	}
	if got.MainClass != want.MainClass {
		t.Errorf("MainClass = %q, want %q", got.MainClass, want.MainClass)
	}
}
