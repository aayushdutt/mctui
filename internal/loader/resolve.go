package loader

import (
	"context"
	"fmt"

	"github.com/quasar/mctui/internal/api"
	"github.com/quasar/mctui/internal/core"
	"github.com/quasar/mctui/internal/loader/fabric"
)

// ResolveVersionDetails returns launch-ready version metadata for an instance.
// Vanilla uses Mojang JSON only; Fabric merges Fabric's profile with the parent game version.
func ResolveVersionDetails(ctx context.Context, mojang *api.MojangClient, inst *core.Instance, offline bool) (*core.VersionDetails, error) {
	if mojang == nil || inst == nil {
		return nil, fmt.Errorf("mojang client and instance are required")
	}

	switch ParseKind(inst.Loader) {
	case KindFabric:
		return fabric.ResolveVersion(ctx, mojang, inst, offline)
	case KindVanilla:
		return mojang.ResolveVersionDetails(ctx, inst.Version, offline)
	default:
		return nil, fmt.Errorf("mod loader %q is not supported yet (coming soon)", inst.Loader)
	}
}
