package ui

// home_facets.go holds the disk-backed readers for the home detail panel's
// tabbed facets (Worlds, Resource packs) plus small presentation helpers
// (mod-name cleanup, playtime formatting). These functions do real I/O so they
// are pure (no model state) and MUST be called lazily and cached per instance
// ID on HomeModel — never inside Update's key path. Missing dirs/files are
// guarded to empty results, never errors or panics.

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
	"unicode"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
)

// worldEntry is one save folder under <inst>/.minecraft/saves/.
type worldEntry struct {
	Name    string    // folder name
	ModTime time.Time // folder mtime, for a relative "last modified" hint
}

// resourcePackEntry is one installed resource pack shown in the home detail
// panel. It is either an individual pack the player picked in the in-app
// VanillaTweaks browser (FromCart, with a Category) — these are merged into a
// single VanillaTweaks.zip on disk but listed individually here — or a
// standalone .zip / directory pack the player dropped into the folder manually.
type resourcePackEntry struct {
	Name     string // display label (humanized pack name, or file/folder name)
	Size     int64  // byte size for standalone .zip packs; 0 otherwise
	IsZip    bool   // true for standalone .zip files (size is meaningful)
	Category string // VanillaTweaks category, for FromCart packs; "" otherwise
	FromCart bool   // true when the entry came from the VanillaTweaks cart
}

// readResourcePacks lists the resource packs installed for inst. The in-app
// browser merges every selected VanillaTweaks pack into a single
// VanillaTweaks.zip, so a naive directory listing would show one opaque zip
// instead of the packs the player actually chose. Instead we expand the applied
// cart (.mctui-vanillatweaks.json) into one entry per pack, then append any
// other packs dropped into the folder manually (zips with sizes + directory
// packs), excluding the merged zip itself. Dotfiles are ignored. Nothing
// installed yields nil ("No resource packs yet"). Pure I/O — cache per instance.
func readResourcePacks(inst *core.Instance) []resourcePackEntry {
	if inst == nil || inst.Path == "" {
		return nil
	}

	var out []resourcePackEntry

	// 1. Individual packs from the applied VanillaTweaks cart (what's actually
	//    merged into the zip on disk), one entry each, sorted by category then
	//    pack so the list is stable.
	if sel, err := resourcepacks.LoadSelection(inst); err == nil {
		cats := make([]string, 0, len(sel.Applied))
		for cat := range sel.Applied {
			cats = append(cats, cat)
		}
		sort.Strings(cats)
		for _, cat := range cats {
			names := append([]string(nil), sel.Applied[cat]...)
			sort.Strings(names)
			for _, name := range names {
				out = append(out, resourcePackEntry{
					Name:     humanizePackID(name),
					Category: cat,
					FromCart: true,
				})
			}
		}
	}
	cartEmpty := len(out) == 0

	// 2. Other packs sitting in the folder. Skip the merged VanillaTweaks.zip
	//    (represented by the cart entries above) unless the cart was empty — then
	//    fall back to listing it so an installed pack is never hidden.
	dir := filepath.Join(inst.Path, ".minecraft", "resourcepacks")
	entries, err := os.ReadDir(dir)
	if err != nil {
		return out
	}
	var files []resourcePackEntry
	for _, e := range entries {
		name := e.Name()
		if strings.HasPrefix(name, ".") {
			continue
		}
		if strings.EqualFold(name, resourcepacks.MergedPackFileName) && !cartEmpty {
			continue
		}
		if e.IsDir() {
			files = append(files, resourcePackEntry{Name: name})
			continue
		}
		if !strings.EqualFold(filepath.Ext(name), ".zip") {
			continue
		}
		var size int64
		if fi, err := e.Info(); err == nil {
			size = fi.Size()
		}
		files = append(files, resourcePackEntry{Name: cleanPackName(name), Size: size, IsZip: true})
	}
	sort.Slice(files, func(i, j int) bool {
		return strings.ToLower(files[i].Name) < strings.ToLower(files[j].Name)
	})
	return append(out, files...)
}

// humanizePackID turns a VanillaTweaks pack id into a readable label: it splits
// PascalCase boundaries ("BorderlessGlass" → "Borderless Glass") and treats
// underscores/hyphens as spaces. The catalog's nicer Display name needs a
// network fetch, so the cart's stable id is humanized for the offline panel.
func humanizePackID(id string) string {
	repl := strings.NewReplacer("_", " ", "-", " ")
	id = repl.Replace(id)
	var b strings.Builder
	var prev rune
	for i, r := range id {
		if i > 0 && unicode.IsUpper(r) && (unicode.IsLower(prev) || unicode.IsDigit(prev)) {
			b.WriteRune(' ')
		}
		b.WriteRune(r)
		prev = r
	}
	return strings.Join(strings.Fields(b.String()), " ")
}

// readWorlds lists the save folders under <instPath>/.minecraft/saves/, newest
// first. A missing or unreadable directory yields nil (rendered as "No worlds
// yet"). Pure I/O — cache the result per instance ID on the model.
func readWorlds(instPath string) []worldEntry {
	if instPath == "" {
		return nil
	}
	dir := filepath.Join(instPath, ".minecraft", "saves")
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var out []worldEntry
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		name := e.Name()
		if strings.HasPrefix(name, ".") {
			continue
		}
		mt := time.Time{}
		if fi, err := e.Info(); err == nil {
			mt = fi.ModTime()
		}
		out = append(out, worldEntry{Name: name, ModTime: mt})
	}
	// Newest first; fall back to name for stable ordering on equal/zero mtimes.
	sort.Slice(out, func(i, j int) bool {
		if out[i].ModTime.Equal(out[j].ModTime) {
			return strings.ToLower(out[i].Name) < strings.ToLower(out[j].Name)
		}
		return out[i].ModTime.After(out[j].ModTime)
	})
	return out
}

// cleanModName turns a jar file name into a tidy display label: drops the .jar
// extension and replaces underscores with spaces so it reads in the panel.
func cleanModName(name string) string {
	name = strings.TrimSuffix(name, ".jar")
	name = strings.TrimSuffix(name, ".JAR")
	name = strings.ReplaceAll(name, "_", " ")
	return name
}

// cleanPackName turns a resource-pack file/folder name into a tidy display
// label, mirroring cleanModName: drops a .zip extension (folder packs have
// none) and replaces underscores with spaces. The row's size annotation already
// signals a zip, so the extension carries no information in the list.
func cleanPackName(name string) string {
	if ext := filepath.Ext(name); strings.EqualFold(ext, ".zip") {
		name = name[:len(name)-len(ext)]
	}
	name = strings.ReplaceAll(name, "_", " ")
	return name
}

// formatPlayTime renders a playtime in seconds humanely ("4h 12m", "8m",
// "Never" when zero, "<1m" for a sub-minute non-zero value).
func formatPlayTime(seconds int64) string {
	if seconds <= 0 {
		return "Never"
	}
	d := time.Duration(seconds) * time.Second
	h := int(d.Hours())
	m := int(d.Minutes()) % 60
	switch {
	case h > 0 && m > 0:
		return fmt.Sprintf("%dh %dm", h, m)
	case h > 0:
		return fmt.Sprintf("%dh", h)
	case m > 0:
		return fmt.Sprintf("%dm", m)
	default:
		return "<1m"
	}
}
