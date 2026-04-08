package fabric

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"time"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

const metaBase = "https://meta.fabricmc.net"

var metaHTTP = &http.Client{Timeout: 60 * time.Second}

type loaderMetaEntry struct {
	Loader struct {
		Version string `json:"version"`
		Stable  bool   `json:"stable"`
	} `json:"loader"`
}

// ResolveVersion loads merged Fabric+Mojang version metadata; may set LoaderVer to latest stable Fabric (online).
func ResolveVersion(ctx context.Context, mojang *api.MojangClient, inst *core.Instance, offline bool) (*core.VersionDetails, error) {
	if inst == nil {
		return nil, fmt.Errorf("instance required")
	}
	gameVer := inst.Version
	if gameVer == "" {
		return nil, fmt.Errorf("instance has no Minecraft version")
	}

	loaderVer := inst.LoaderVer
	cacheDir := mojang.VersionCacheDir()

	if offline {
		if loaderVer == "" {
			return nil, fmt.Errorf("fabric offline needs a saved loader version; connect once after choosing Fabric")
		}
		cacheFile := mergedProfileCacheFile(cacheDir, gameVer, loaderVer)
		data, err := os.ReadFile(cacheFile)
		if err != nil {
			return nil, fmt.Errorf("offline fabric launch needs cached profile %s: %w", filepath.Base(cacheFile), err)
		}
		var details core.VersionDetails
		if err := json.Unmarshal(data, &details); err != nil {
			return nil, fmt.Errorf("read fabric cache: %w", err)
		}
		return &details, nil
	}

	parent, err := mojang.ResolveVersionDetails(ctx, gameVer, false)
	if err != nil {
		return nil, fmt.Errorf("vanilla version %s: %w", gameVer, err)
	}

	if loaderVer == "" {
		v, err := pickStableLoaderVersion(ctx, gameVer)
		if err != nil {
			return nil, err
		}
		loaderVer = v
		inst.LoaderVer = loaderVer
	}

	cacheFile := mergedProfileCacheFile(cacheDir, gameVer, loaderVer)
	if data, err := os.ReadFile(cacheFile); err == nil {
		var details core.VersionDetails
		if json.Unmarshal(data, &details) == nil && details.MainClass != "" {
			return &details, nil
		}
	}

	profileURL := fmt.Sprintf("%s/v2/versions/loader/%s/%s/profile/json",
		metaBase, url.PathEscape(gameVer), url.PathEscape(loaderVer))
	profileJSON, err := fetchBytes(ctx, profileURL)
	if err != nil {
		return nil, fmt.Errorf("fabric profile: %w", err)
	}

	merged, err := MergeProfile(parent, profileJSON)
	if err != nil {
		return nil, err
	}

	merged.ID = parent.ID

	if err := saveMergedCache(cacheFile, merged); err != nil {
		log.Printf("fabric: could not save merged profile cache %s: %v", filepath.Base(cacheFile), err)
	}

	return merged, nil
}

func pickStableLoaderVersion(ctx context.Context, gameVersion string) (string, error) {
	u := fmt.Sprintf("%s/v2/versions/loader/%s", metaBase, url.PathEscape(gameVersion))
	body, err := fetchBytes(ctx, u)
	if err != nil {
		return "", fmt.Errorf("fabric loader list: %w", err)
	}
	var entries []loaderMetaEntry
	if err := json.Unmarshal(body, &entries); err != nil {
		return "", fmt.Errorf("decode fabric loader list: %w", err)
	}
	for _, e := range entries {
		if e.Loader.Stable && e.Loader.Version != "" {
			return e.Loader.Version, nil
		}
	}
	if len(entries) > 0 && entries[0].Loader.Version != "" {
		return entries[0].Loader.Version, nil
	}
	return "", fmt.Errorf("no fabric loader for Minecraft %s", gameVersion)
}

func fetchBytes(ctx context.Context, rawURL string) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, rawURL, nil)
	if err != nil {
		return nil, err
	}
	resp, err := metaHTTP.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		b, _ := io.ReadAll(io.LimitReader(resp.Body, 2048))
		return nil, fmt.Errorf("%s: status %d: %s", rawURL, resp.StatusCode, string(b))
	}
	return io.ReadAll(resp.Body)
}

func saveMergedCache(path string, merged *core.VersionDetails) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	data, err := json.Marshal(merged)
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o644)
}
