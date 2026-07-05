package ui

import (
	"hash/fnv"

	"github.com/charmbracelet/lipgloss"
)

// swatch.go gives every instance a tiny identity: a 2-cell half-block glyph
// colored by a stable hash of its ID from a small Minecraft-block palette. The
// colors are fixed hexes (block colors, not theme roles) and are painted as
// FOREGROUND only with no text on top, so the swatch carries no contrast risk in
// any theme — it is pure decoration, part of Phase 3's personality budget.

// swatchColors is the Minecraft-ish block palette: grass, stone, redstone, gold,
// diamond, obsidian. Kept to six so the hash spreads recognizably.
var swatchColors = []lipgloss.Color{
	lipgloss.Color("#5D9C3C"), // grass
	lipgloss.Color("#8A8A8A"), // stone
	lipgloss.Color("#C0392B"), // redstone
	lipgloss.Color("#E6B422"), // gold
	lipgloss.Color("#4EC9E6"), // diamond
	lipgloss.Color("#4A3B6B"), // obsidian
}

// swatchColorForID maps an instance ID to a stable palette color via FNV-1a, so
// the same instance always wears the same block color across sessions.
func swatchColorForID(id string) lipgloss.Color {
	h := fnv.New32a()
	_, _ = h.Write([]byte(id))
	return swatchColors[h.Sum32()%uint32(len(swatchColors))]
}

// InstanceSwatch returns the 2-cell "▄▀" block glyph for an instance, colored by
// [swatchColorForID]. A blank ID yields a neutral faint swatch so callers can
// always render a fixed-width 2-cell gutter.
func InstanceSwatch(id string) string {
	if id == "" {
		return lipgloss.NewStyle().Foreground(Active.TextFaint).Render("▄▀")
	}
	return lipgloss.NewStyle().Foreground(swatchColorForID(id)).Render("▄▀")
}
