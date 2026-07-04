//go:build live

package fabric

import (
	"context"
	"fmt"
	"net/url"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
)

// Live test hits the REAL Fabric meta host via the package's own resolver/
// fetchBytes and the production metaBase (NOT overridden). Gated behind `live`.

func TestLive_FabricProfile(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	const gameVer = "1.21"

	loaderVer, err := pickStableLoaderVersion(ctx, gameVer)
	if err != nil {
		t.Fatalf("pickStableLoaderVersion: %v", err)
	}
	if loaderVer == "" {
		t.Fatal("empty loader version")
	}

	profileURL := fmt.Sprintf("%s/v2/versions/loader/%s/%s/profile/json",
		metaBase, url.PathEscape(gameVer), url.PathEscape(loaderVer))
	profileJSON, err := fetchBytes(ctx, profileURL)
	if err != nil {
		t.Fatalf("fetchBytes(%s): %v", profileURL, err)
	}

	merged, err := MergeProfile(&core.VersionDetails{ID: gameVer}, profileJSON)
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
