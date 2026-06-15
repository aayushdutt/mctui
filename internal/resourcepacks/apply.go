package resourcepacks

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/aayushdutt/mctui/internal/core"
)

// optionsEntry is the resourcePacks array entry referencing the merged pack.
// Minecraft prefixes filesystem packs with "file/".
const optionsEntry = "file/" + MergedPackFileName

// defaultOptionsTxt is the minimal options.txt written when none exists. It only
// seeds the resourcePacks line we manage; Minecraft fills in the rest on launch.
const defaultOptionsTxt = "resourcePacks:[\"vanilla\",\"" + optionsEntry + "\"]\n"

// ApplyResult summarizes a BuildAndApply run for UI status lines.
type ApplyResult struct {
	// PackCount is the number of packs merged into the zip.
	PackCount int
	// DestPath is the absolute path the merged zip was written to.
	DestPath string
	// EnabledInOptions is true when "file/VanillaTweaks.zip" was ensured present
	// in options.txt resourcePacks (whether newly added or already present).
	EnabledInOptions bool
	// Version is the catalog version the pack was built against.
	Version string
}

// BuildAndApply builds the merged resource pack from the cart, downloads it to
// <instance>/.minecraft/resourcepacks/VanillaTweaks.zip, snapshots the cart as
// applied, and ensures it is enabled in options.txt. It is the one-call entry the
// UI invokes from a command goroutine.
func (s *Service) BuildAndApply(ctx context.Context, inst *core.Instance, sel *Selection) (*ApplyResult, error) {
	if s == nil || s.VT == nil {
		return nil, fmt.Errorf("vanilla tweaks client required")
	}
	if inst == nil {
		return nil, fmt.Errorf("instance required")
	}
	if sel == nil {
		return nil, fmt.Errorf("selection required")
	}

	selMap := sel.BuildSelectionMap()
	if len(selMap) == 0 {
		return nil, fmt.Errorf("cart is empty: select at least one resource pack")
	}

	version := sel.Version
	if version == "" {
		version, _ = CatalogVersionFor(inst)
	}

	// 1. Build the merged zip server-side.
	downloadURL, err := s.VT.BuildResourcePackZip(ctx, selMap, version)
	if err != nil {
		return nil, fmt.Errorf("build resource pack: %w", err)
	}

	// 2. Download it to the stable destination (atomic overwrite handled by the
	//    download layer's tmp+rename).
	dir := ResourcePacksDir(inst)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, fmt.Errorf("create resourcepacks dir: %w", err)
	}
	dest := MergedPackPath(inst)
	if err := s.VT.DownloadResourcePackZip(ctx, downloadURL, dest); err != nil {
		return nil, fmt.Errorf("download merged pack: %w", err)
	}

	// 3. Enable in options.txt (idempotent).
	enabled, err := EnableInOptionsTxt(inst)
	if err != nil {
		return nil, fmt.Errorf("enable in options.txt: %w", err)
	}

	// 4. Record applied selection so Dirty() reflects this build, and persist.
	sel.MarkApplied()
	if err := SaveSelection(inst, sel); err != nil {
		return nil, fmt.Errorf("save cart: %w", err)
	}

	return &ApplyResult{
		PackCount:        countSelection(selMap),
		DestPath:         dest,
		EnabledInOptions: enabled,
		Version:          version,
	}, nil
}

// countSelection totals the pack ids across all categories.
func countSelection(m map[string][]string) int {
	n := 0
	for _, names := range m {
		n += len(names)
	}
	return n
}

// OptionsTxtPath is the absolute path of options.txt for an instance.
func OptionsTxtPath(inst *core.Instance) string {
	if inst == nil {
		return ""
	}
	return optionsTxtPath(inst)
}

// EnableInOptionsTxt ensures "file/<MergedPackFileName>" is present in the
// resourcePacks JSON array in options.txt. It creates options.txt with a sane
// default if absent and is idempotent. Returns whether the entry is now present.
func EnableInOptionsTxt(inst *core.Instance) (bool, error) {
	if inst == nil {
		return false, fmt.Errorf("instance required")
	}
	p := optionsTxtPath(inst)

	data, err := os.ReadFile(p)
	if err != nil {
		if !os.IsNotExist(err) {
			return false, err
		}
		// Absent: create a minimal valid file enabling our pack.
		if err := os.MkdirAll(filepath.Dir(p), 0755); err != nil {
			return false, err
		}
		if err := atomicWriteFile(p, []byte(defaultOptionsTxt)); err != nil {
			return false, err
		}
		return true, nil
	}

	updated, changed := ensureEntryInOptions(string(data))
	if changed {
		if err := atomicWriteFile(p, []byte(updated)); err != nil {
			return false, err
		}
	}
	return true, nil
}

// IsEnabledInOptionsTxt reports whether the merged pack is currently listed in
// options.txt resourcePacks.
func IsEnabledInOptionsTxt(inst *core.Instance) bool {
	if inst == nil {
		return false
	}
	data, err := os.ReadFile(optionsTxtPath(inst))
	if err != nil {
		return false
	}
	entries, found := parseResourcePacksLine(string(data))
	if !found {
		return false
	}
	for _, e := range entries {
		if e == optionsEntry {
			return true
		}
	}
	return false
}

// ensureEntryInOptions returns options.txt content with optionsEntry guaranteed
// present in the resourcePacks:[...] line, preserving other entries and key
// order. If the line is missing entirely it is appended. The boolean reports
// whether the content was modified.
func ensureEntryInOptions(content string) (string, bool) {
	lines := strings.Split(content, "\n")
	for i, line := range lines {
		if !strings.HasPrefix(strings.TrimSpace(line), "resourcePacks:") {
			continue
		}
		entries, ok := parseResourcePacksValue(strings.TrimSpace(line))
		if !ok {
			// Malformed value: rebuild the line with a sane default that keeps
			// vanilla and enables our pack.
			lines[i] = "resourcePacks:" + encodeEntries([]string{"vanilla", optionsEntry})
			return strings.Join(lines, "\n"), true
		}
		for _, e := range entries {
			if e == optionsEntry {
				return content, false // already present
			}
		}
		entries = append(entries, optionsEntry)
		lines[i] = "resourcePacks:" + encodeEntries(entries)
		return strings.Join(lines, "\n"), true
	}

	// No resourcePacks line at all: append one.
	appended := "resourcePacks:" + encodeEntries([]string{"vanilla", optionsEntry})
	if content == "" {
		return appended + "\n", true
	}
	if strings.HasSuffix(content, "\n") {
		return content + appended + "\n", true
	}
	return content + "\n" + appended + "\n", true
}

// parseResourcePacksLine extracts the entries from the resourcePacks line in a
// full options.txt body. found is false when no parseable line exists.
func parseResourcePacksLine(content string) (entries []string, found bool) {
	for _, line := range strings.Split(content, "\n") {
		t := strings.TrimSpace(line)
		if !strings.HasPrefix(t, "resourcePacks:") {
			continue
		}
		if e, ok := parseResourcePacksValue(t); ok {
			return e, true
		}
		return nil, false
	}
	return nil, false
}

// parseResourcePacksValue parses a single "resourcePacks:[...]" line into its
// entries. The value is a JSON array of strings. ok is false when the value is
// malformed.
func parseResourcePacksValue(line string) (entries []string, ok bool) {
	const key = "resourcePacks:"
	if !strings.HasPrefix(line, key) {
		return nil, false
	}
	value := strings.TrimSpace(line[len(key):])
	if value == "" {
		return nil, false
	}
	var arr []string
	if err := json.Unmarshal([]byte(value), &arr); err != nil {
		return nil, false
	}
	return arr, true
}

// encodeEntries renders entries as the JSON array Minecraft writes (compact,
// double-quoted, comma-separated, no spaces).
func encodeEntries(entries []string) string {
	parts := make([]string, len(entries))
	for i, e := range entries {
		b, _ := json.Marshal(e)
		parts[i] = string(b)
	}
	return "[" + strings.Join(parts, ",") + "]"
}

// atomicWriteFile writes data to path via a tmp file + rename.
func atomicWriteFile(path string, data []byte) error {
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// optionsTxtPath is the unexported path helper; OptionsTxtPath wraps it.
func optionsTxtPath(inst *core.Instance) string {
	if inst == nil {
		return ""
	}
	return filepath.Join(inst.Path, ".minecraft", "options.txt")
}
