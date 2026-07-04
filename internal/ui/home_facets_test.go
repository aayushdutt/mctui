package ui

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
)

func TestReadWorlds(t *testing.T) {
	inst := t.TempDir()
	saves := filepath.Join(inst, ".minecraft", "saves")
	if err := os.MkdirAll(filepath.Join(saves, "Alpha"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(saves, "Beta"), 0o755); err != nil {
		t.Fatal(err)
	}
	// A regular file and a dotfolder must be ignored.
	if err := os.WriteFile(filepath.Join(saves, "notes.txt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(saves, ".hidden"), 0o755); err != nil {
		t.Fatal(err)
	}

	got := readWorlds(inst)
	if len(got) != 2 {
		t.Fatalf("readWorlds returned %d entries, want 2: %+v", len(got), got)
	}
	names := map[string]bool{}
	for _, w := range got {
		names[w.Name] = true
	}
	if !names["Alpha"] || !names["Beta"] {
		t.Fatalf("missing expected worlds, got %+v", got)
	}
}

func TestReadWorldsMissingDir(t *testing.T) {
	if got := readWorlds(t.TempDir()); got != nil {
		t.Fatalf("expected nil for missing saves dir, got %+v", got)
	}
	if got := readWorlds(""); got != nil {
		t.Fatalf("expected nil for empty path, got %+v", got)
	}
}

func TestReadResourcePacks(t *testing.T) {
	dir := t.TempDir()
	rpDir := filepath.Join(dir, ".minecraft", "resourcepacks")
	if err := os.MkdirAll(rpDir, 0o755); err != nil {
		t.Fatal(err)
	}
	// A manually-added .zip pack and one directory pack.
	if err := os.WriteFile(filepath.Join(rpDir, "Faithful.zip"), []byte("xxxxxxxx"), 0o644); err != nil {
		t.Fatal(err)
	}
	// The merged VanillaTweaks.zip exists on disk but, with no cart, must fall
	// back to a single listed entry (never silently hidden).
	if err := os.WriteFile(filepath.Join(rpDir, "VanillaTweaks.zip"), []byte("xxxx"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(rpDir, "UnzippedPack"), 0o755); err != nil {
		t.Fatal(err)
	}
	// A non-zip file and a dotfile must be ignored.
	if err := os.WriteFile(filepath.Join(rpDir, "notes.txt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rpDir, ".DS_Store"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}

	got := readResourcePacks(&core.Instance{Path: dir})
	if len(got) != 3 {
		t.Fatalf("readResourcePacks returned %d entries, want 3: %+v", len(got), got)
	}
	byName := map[string]resourcePackEntry{}
	for _, p := range got {
		byName[p.Name] = p
	}
	// The .zip extension is dropped for display (mirrors mod-name cleanup).
	if !byName["Faithful"].IsZip || byName["Faithful"].Size != 8 {
		t.Fatalf("Faithful not read as a zip with size 8: %+v", byName["Faithful"])
	}
	if _, ok := byName["VanillaTweaks"]; !ok {
		t.Fatalf("merged VanillaTweaks.zip should fall back to a listed entry when no cart: %+v", got)
	}
	if byName["UnzippedPack"].IsZip {
		t.Fatalf("UnzippedPack should be a directory pack: %+v", byName["UnzippedPack"])
	}
}

// TestReadResourcePacksExpandsCart verifies the in-app VanillaTweaks packs are
// listed individually (from the applied cart) instead of as the single merged
// VanillaTweaks.zip on disk.
func TestReadResourcePacksExpandsCart(t *testing.T) {
	dir := t.TempDir()
	inst := &core.Instance{Path: dir}
	rpDir := filepath.Join(dir, ".minecraft", "resourcepacks")
	if err := os.MkdirAll(rpDir, 0o755); err != nil {
		t.Fatal(err)
	}
	// The merged pack on disk + a manually-added standalone pack.
	if err := os.WriteFile(filepath.Join(rpDir, resourcepacks.MergedPackFileName), []byte("xxxx"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rpDir, "Faithful.zip"), []byte("xx"), 0o644); err != nil {
		t.Fatal(err)
	}
	// An applied cart selecting two packs across categories.
	sel := resourcepacks.NewSelection("1.21")
	sel.Packs = map[string][]string{
		"Aesthetic": {"BorderlessGlass"},
		"Terrain":   {"BlueNetherrack"},
	}
	sel.MarkApplied()
	if err := resourcepacks.SaveSelection(inst, sel); err != nil {
		t.Fatal(err)
	}

	got := readResourcePacks(inst)

	byName := map[string]resourcePackEntry{}
	for _, p := range got {
		byName[p.Name] = p
	}
	// Cart packs are listed individually, humanized, with their category — not as
	// the merged zip.
	bg, ok := byName["Borderless Glass"]
	if !ok || !bg.FromCart || bg.Category != "Aesthetic" {
		t.Fatalf("expected humanized cart pack 'Borderless Glass' (Aesthetic): %+v", got)
	}
	if _, ok := byName["Blue Netherrack"]; !ok {
		t.Fatalf("expected humanized cart pack 'Blue Netherrack': %+v", got)
	}
	// The merged zip must NOT appear as its own entry when the cart is non-empty.
	if _, ok := byName["VanillaTweaks"]; ok {
		t.Fatalf("merged VanillaTweaks.zip should be hidden when the cart expands it: %+v", got)
	}
	// The manually-added standalone pack is still listed.
	if !byName["Faithful"].IsZip {
		t.Fatalf("manually-added Faithful.zip should still be listed: %+v", got)
	}
}

func TestReadResourcePacksMissingDir(t *testing.T) {
	if got := readResourcePacks(&core.Instance{Path: t.TempDir()}); got != nil {
		t.Fatalf("expected nil for missing resourcepacks dir, got %+v", got)
	}
	if got := readResourcePacks(&core.Instance{Path: ""}); got != nil {
		t.Fatalf("expected nil for empty path, got %+v", got)
	}
	if got := readResourcePacks(nil); got != nil {
		t.Fatalf("expected nil for nil instance, got %+v", got)
	}
}

func TestFormatPlayTime(t *testing.T) {
	cases := []struct {
		in   int64
		want string
	}{
		{0, "Never"},
		{-5, "Never"},
		{30, "<1m"},
		{8 * 60, "8m"},
		{4*3600 + 12*60, "4h 12m"},
		{2 * 3600, "2h"},
	}
	for _, c := range cases {
		if got := formatPlayTime(c.in); got != c.want {
			t.Errorf("formatPlayTime(%d) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestCleanModName(t *testing.T) {
	if got := cleanModName("sodium_fabric_0.5.jar"); got != "sodium fabric 0.5" {
		t.Errorf("cleanModName = %q", got)
	}
	if got := cleanModName("lithium.jar"); got != "lithium" {
		t.Errorf("cleanModName = %q", got)
	}
}
