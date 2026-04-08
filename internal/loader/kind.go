// Package loader resolves Minecraft version metadata for instances with mod loaders.
// Each loader implementation (Fabric, future Forge/Quilt/…) lives in a subpackage.
package loader

import "strings"

// Kind identifies a mod loader backend.
type Kind string

const (
	KindVanilla Kind = "vanilla"
	KindFabric  Kind = "fabric"
	KindForge   Kind = "forge"
	KindQuilt   Kind = "quilt"
)

// ParseKind normalizes instance.Instance.Loader (and empty) to a Kind.
func ParseKind(loader string) Kind {
	switch strings.ToLower(strings.TrimSpace(loader)) {
	case "", "vanilla":
		return KindVanilla
	case "fabric":
		return KindFabric
	case "forge":
		return KindForge
	case "quilt":
		return KindQuilt
	default:
		return Kind(loader)
	}
}
