package resourcepacks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

// cartFileName is the persisted per-instance cart, stored alongside the merged
// pack under .minecraft/resourcepacks.
const cartFileName = ".mctui-vanillatweaks.json"

// Selection is the persistent per-instance cart: a map of category DISPLAY name
// -> selected pack "name" ids. It also records the catalog version it was built
// against and the selection that was last applied (built + enabled), so the UI
// can show a Dirty state.
//
// The Selection field shape (category -> []name) is exactly the body the build
// endpoint expects, so BuildSelectionMap is a near-identity projection.
type Selection struct {
	// Version is the Vanilla Tweaks catalog version this cart targets.
	Version string `json:"version"`
	// Packs maps category display name -> selected pack ids (cart contents).
	Packs map[string][]string `json:"packs"`
	// Applied is the snapshot of Packs at the last successful BuildAndApply.
	// nil means "never applied".
	Applied map[string][]string `json:"applied,omitempty"`
}

// SwapResult reports incompatibility resolution performed by Toggle: when adding
// a pack forces other (incompatible) packs out of the cart, they are listed in
// Removed. Added is the pack that was toggled on (empty when Toggle turned a pack
// off).
type SwapResult struct {
	// Added is the pack id newly enabled (empty if the toggle disabled a pack).
	Added string
	// Category is the display category of the toggled pack.
	Category string
	// Removed lists packs (ids) evicted to satisfy incompatibilities.
	Removed []string
	// Enabled is the resulting state of the toggled pack after the operation.
	Enabled bool
}

// cartPath is the absolute path of the persisted cart for an instance.
func cartPath(inst *core.Instance) string {
	if inst == nil {
		return ""
	}
	return filepath.Join(ResourcePacksDir(inst), cartFileName)
}

// LoadSelection reads the persisted cart, or returns an empty Selection if absent.
// A missing or unreadable/corrupt file yields a fresh empty cart rather than an
// error, so the screen can always open.
func LoadSelection(inst *core.Instance) (*Selection, error) {
	if inst == nil {
		return &Selection{Packs: map[string][]string{}}, nil
	}
	data, err := os.ReadFile(cartPath(inst))
	if err != nil {
		if os.IsNotExist(err) {
			return &Selection{Packs: map[string][]string{}}, nil
		}
		return nil, err
	}
	var sel Selection
	if err := json.Unmarshal(data, &sel); err != nil {
		// Corrupt cart: don't block the screen, start fresh.
		return &Selection{Packs: map[string][]string{}}, nil
	}
	if sel.Packs == nil {
		sel.Packs = map[string][]string{}
	}
	return &sel, nil
}

// SaveSelection writes the cart atomically under the instance resourcepacks dir.
func SaveSelection(inst *core.Instance, sel *Selection) error {
	if inst == nil || sel == nil {
		return fmt.Errorf("instance and selection required")
	}
	dir := ResourcePacksDir(inst)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(sel, "", "  ")
	if err != nil {
		return err
	}
	p := cartPath(inst)
	tmp := p + ".tmp"
	if err := os.WriteFile(tmp, data, 0644); err != nil {
		return err
	}
	return os.Rename(tmp, p)
}

// NewSelection returns an empty cart targeting the given catalog version.
func NewSelection(version string) *Selection {
	return &Selection{Version: version, Packs: map[string][]string{}}
}

// Has reports whether pack name is currently in the cart under category.
func (s *Selection) Has(category, name string) bool {
	if s == nil {
		return false
	}
	for _, n := range s.Packs[category] {
		if n == name {
			return true
		}
	}
	return false
}

// AppliedHas reports whether pack name under category is in the last-applied
// (built) snapshot. Used to distinguish already-built packs from pending cart
// changes in the UI.
func (s *Selection) AppliedHas(category, name string) bool {
	if s == nil {
		return false
	}
	for _, n := range s.Applied[category] {
		if n == name {
			return true
		}
	}
	return false
}

// hasAnywhere reports whether name is selected under any category.
func (s *Selection) hasAnywhere(name string) bool {
	if s == nil {
		return false
	}
	for _, names := range s.Packs {
		for _, n := range names {
			if n == name {
				return true
			}
		}
	}
	return false
}

// BlockerFor returns the id of a pack currently in the cart that is incompatible
// with name (resolved bidirectionally via the catalog), or ("", false) when name
// is already selected or nothing in the cart conflicts. This keeps all
// incompatibility reasoning in the domain layer — the UI only renders the result.
func (s *Selection) BlockerFor(cat *api.ResourcePackCatalog, name string) (blockerID string, ok bool) {
	if s == nil || s.hasAnywhere(name) {
		return "", false
	}
	conflicts := incompatibleIDs(cat, name)
	if len(conflicts) == 0 {
		return "", false
	}
	for _, names := range s.Packs {
		for _, n := range names {
			if conflicts[n] {
				return n, true
			}
		}
	}
	return "", false
}

// Count returns the total number of packs currently in the cart.
func (s *Selection) Count() int {
	if s == nil {
		return 0
	}
	total := 0
	for _, names := range s.Packs {
		total += len(names)
	}
	return total
}

// CountInCategory returns the number of cart packs under a category.
func (s *Selection) CountInCategory(category string) int {
	if s == nil {
		return 0
	}
	return len(s.Packs[category])
}

// add inserts name under category if not already present.
func (s *Selection) add(category, name string) {
	if s.Packs == nil {
		s.Packs = map[string][]string{}
	}
	if s.Has(category, name) {
		return
	}
	s.Packs[category] = append(s.Packs[category], name)
}

// Toggle adds or removes pack (category, name) from the cart. When enabling a
// pack, packs it is incompatible with (per the catalog) are removed and reported
// in the returned SwapResult. The catalog is consulted to resolve incompatibility
// in both directions: a pack X is evicted when adding Y if X lists Y, OR Y lists
// X.
func (s *Selection) Toggle(cat *api.ResourcePackCatalog, category, name string) SwapResult {
	res := SwapResult{Category: category}
	if s == nil {
		return res
	}
	if s.Packs == nil {
		s.Packs = map[string][]string{}
	}

	// Toggling off: just remove.
	if s.Has(category, name) {
		s.Remove(category, name)
		res.Enabled = false
		return res
	}

	// Toggling on: resolve incompatibilities first, then add.
	res.Added = name
	res.Enabled = true

	// Build the set of pack ids that conflict with `name`, in both directions.
	conflicts := incompatibleIDs(cat, name)

	if len(conflicts) > 0 {
		var removed []string
		for c, names := range s.Packs {
			kept := names[:0:0]
			for _, n := range names {
				if conflicts[n] {
					removed = append(removed, n)
					continue
				}
				kept = append(kept, n)
			}
			if len(kept) == 0 {
				delete(s.Packs, c)
			} else {
				s.Packs[c] = kept
			}
		}
		if len(removed) > 0 {
			sort.Strings(removed)
			res.Removed = removed
		}
	}

	s.add(category, name)
	return res
}

// incompatibleIDs returns the set of pack ids that cannot coexist with `name`,
// resolved bidirectionally from the catalog: any pack that lists `name` in its
// Incompatible[], plus every id `name` itself lists. The target `name` is never
// included in the returned set.
func incompatibleIDs(cat *api.ResourcePackCatalog, name string) map[string]bool {
	conflicts := map[string]bool{}
	if cat == nil {
		return conflicts
	}
	walkPacks(cat.Categories, func(p api.RPPack) {
		switch {
		case p.Name == name:
			// Outgoing edges: packs `name` declares it is incompatible with.
			for _, inc := range p.Incompatible {
				if inc != name {
					conflicts[inc] = true
				}
			}
		default:
			// Incoming edges: packs that declare incompatibility with `name`.
			for _, inc := range p.Incompatible {
				if inc == name {
					conflicts[p.Name] = true
					break
				}
			}
		}
	})
	delete(conflicts, name)
	return conflicts
}

// walkPacks visits every pack across categories and nested subcategories.
func walkPacks(cats []api.RPCategory, fn func(api.RPPack)) {
	for _, c := range cats {
		for _, p := range c.Packs {
			fn(p)
		}
		if len(c.Categories) > 0 {
			walkPacks(c.Categories, fn)
		}
	}
}

// Remove drops a pack from the cart (no-op if absent).
func (s *Selection) Remove(category, name string) {
	if s == nil || s.Packs == nil {
		return
	}
	names, ok := s.Packs[category]
	if !ok {
		return
	}
	out := names[:0:0]
	for _, n := range names {
		if n != name {
			out = append(out, n)
		}
	}
	if len(out) == 0 {
		delete(s.Packs, category)
	} else {
		s.Packs[category] = out
	}
}

// Clear empties the cart (leaving Applied untouched).
func (s *Selection) Clear() {
	s.Packs = map[string][]string{}
}

// BuildSelectionMap projects the cart into the category->[]name map the build
// endpoint expects, omitting empty categories. Pack ids within a category are
// sorted for a stable request body.
func (s *Selection) BuildSelectionMap() map[string][]string {
	if s == nil {
		return map[string][]string{}
	}
	out := make(map[string][]string, len(s.Packs))
	for cat, names := range s.Packs {
		if len(names) == 0 {
			continue
		}
		cp := append([]string(nil), names...)
		sort.Strings(cp)
		out[cat] = cp
	}
	return out
}

// Dirty reports whether the cart differs from the last-applied snapshot (i.e.
// there are unbuilt/unapplied changes). An empty cart that was never applied is
// not dirty.
func (s *Selection) Dirty() bool {
	if s == nil {
		return false
	}
	return !selectionsEqual(s.Packs, s.Applied)
}

// Clone returns a deep copy of the selection: Packs and Applied are duplicated
// so the copy shares no slices/maps with the original. This lets a command
// goroutine own a private selection (e.g. for BuildAndApply) without racing the
// bubbletea Update/View goroutine that holds the live one.
func (s *Selection) Clone() *Selection {
	if s == nil {
		return nil
	}
	return &Selection{
		Version: s.Version,
		Packs:   cloneSelection(s.Packs),
		Applied: cloneSelection(s.Applied),
	}
}

// MarkApplied snapshots the current cart contents as the last-applied state.
func (s *Selection) MarkApplied() {
	if s == nil {
		return
	}
	s.Applied = cloneSelection(s.Packs)
}

// cloneSelection deep-copies a category->ids map, dropping empty categories.
func cloneSelection(in map[string][]string) map[string][]string {
	out := make(map[string][]string, len(in))
	for cat, names := range in {
		if len(names) == 0 {
			continue
		}
		out[cat] = append([]string(nil), names...)
	}
	return out
}

// selectionsEqual compares two category->ids maps for set-equality, ignoring
// pack order within a category and treating empty categories as absent.
func selectionsEqual(a, b map[string][]string) bool {
	an := normalizeForCompare(a)
	bn := normalizeForCompare(b)
	if len(an) != len(bn) {
		return false
	}
	for cat, aset := range an {
		bset, ok := bn[cat]
		if !ok || len(aset) != len(bset) {
			return false
		}
		for name := range aset {
			if !bset[name] {
				return false
			}
		}
	}
	return true
}

// normalizeForCompare turns a category->ids map into category->set, dropping
// empty categories so {} and {"X":[]} compare equal.
func normalizeForCompare(in map[string][]string) map[string]map[string]bool {
	out := map[string]map[string]bool{}
	for cat, names := range in {
		if len(names) == 0 {
			continue
		}
		set := make(map[string]bool, len(names))
		for _, n := range names {
			set[n] = true
		}
		out[cat] = set
	}
	return out
}
