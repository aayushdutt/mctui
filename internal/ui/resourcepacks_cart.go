package ui

import (
	"fmt"
	"sort"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
)

// This file is the cart/selection bridge: it builds the pack and cart list rows
// from the live Selection (with applied/blocked markers), toggles membership,
// and renders incompatibility/swap results. Incompatibility *reasoning* lives in
// the domain layer (resourcepacks.Selection); here we only project it to rows.

// rebuildPackItems repopulates the pack list for the currently selected category
// (or every pack, in global search), marking cart membership, applied state, and
// incompatibility blockers.
func (m *ResourcePacksModel) rebuildPackItems() {
	idx := m.packs.Index()
	m.currentPacks = m.currentPacks[:0]
	items := []list.Item{}

	if m.searchMode {
		// Flatten every pack across the catalog so the list filter searches all of
		// them; each item carries its own category for the jump.
		for _, c := range allCategories(m.catalog) {
			for _, p := range c.Packs {
				ref := RPPackRef{Category: c.Category, Pack: p}
				m.currentPacks = append(m.currentPacks, ref)
				items = append(items, rpPackItem{
					pack:         ref,
					inCart:       m.sel.Has(c.Category, p.Name),
					applied:      m.sel.AppliedHas(c.Category, p.Name),
					blocker:      m.incompatBlocker(p),
					showCategory: true,
				})
			}
		}
	} else if cat, ok := m.selectedCategory(); ok {
		for _, p := range cat.category.Packs {
			ref := RPPackRef{Category: cat.category.Category, Pack: p}
			m.currentPacks = append(m.currentPacks, ref)
			items = append(items, rpPackItem{
				pack:    ref,
				inCart:  m.sel.Has(cat.category.Category, p.Name),
				applied: m.sel.AppliedHas(cat.category.Category, p.Name),
				blocker: m.incompatBlocker(p),
			})
		}
	}
	m.packs.SetItems(items)
	if idx >= len(items) {
		idx = len(items) - 1
	}
	if idx < 0 {
		idx = 0
	}
	if len(items) > 0 {
		m.packs.Select(idx)
	}
	// The selected category (warning?) and cart size may have changed; re-budget
	// the pack/cart list viewports so the in-panel CTA/warning blocks stay within
	// the screen height.
	m.applyPaneListHeights()
}

// incompatBlocker returns the display label of a cart pack that is incompatible
// with p, or "" when nothing blocks it. The incompatibility reasoning lives in
// the domain layer (Selection.BlockerFor); here we only resolve id → label.
func (m *ResourcePacksModel) incompatBlocker(p api.RPPack) string {
	id, ok := m.sel.BlockerFor(m.catalog, p.Name)
	if !ok {
		return ""
	}
	if label, found := m.anyPackLabel(id); found {
		return label
	}
	return id
}

// cartPacks resolves the cart selection into RPPackRefs via the catalog.
func (m *ResourcePacksModel) cartPacks() []RPPackRef {
	var out []RPPackRef
	sel := m.sel.BuildSelectionMap()
	if sel == nil {
		// Fall back to the raw cart map (BuildSelectionMap may be empty pre-apply).
		sel = m.sel.Packs
	}
	for category, names := range sel {
		nameSet := map[string]bool{}
		for _, n := range names {
			nameSet[n] = true
		}
		for _, cat := range allCategories(m.catalog) {
			if cat.Category != category {
				continue
			}
			for _, p := range cat.Packs {
				if nameSet[p.Name] {
					out = append(out, RPPackRef{Category: category, Pack: p})
				}
			}
		}
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Category != out[j].Category {
			return out[i].Category < out[j].Category
		}
		return displayOf(out[i].Pack) < displayOf(out[j].Pack)
	})
	return out
}

// rebuildCartItems repopulates the cart pane from the current selection.
func (m *ResourcePacksModel) rebuildCartItems() {
	idx := m.cart.Index()
	refs := m.cartPacks()
	items := make([]list.Item, 0, len(refs))
	for _, r := range refs {
		items = append(items, rpCartItem{pack: r, applied: m.sel.AppliedHas(r.Category, r.Pack.Name)})
	}
	m.cart.SetItems(items)
	if idx >= len(items) {
		idx = len(items) - 1
	}
	if idx < 0 {
		idx = 0
	}
	if len(items) > 0 {
		m.cart.Select(idx)
	}
}

// selectedPackRef returns the pack highlighted in the focused packs/cart pane.
func (m *ResourcePacksModel) selectedPackRef() (RPPackRef, bool) {
	switch m.rpFocus {
	case rpPanelPacks:
		if it, ok := m.packs.SelectedItem().(rpPackItem); ok {
			return it.pack, true
		}
	case rpPanelCart:
		if it, ok := m.cart.SelectedItem().(rpCartItem); ok {
			return it.pack, true
		}
	}
	return RPPackRef{}, false
}

// toggleSelectedPack adds/removes the highlighted pack in the active pane.
func (m *ResourcePacksModel) toggleSelectedPack() tea.Cmd {
	ref, ok := m.selectedPackRef()
	if !ok {
		return nil
	}
	return m.toggleRef(ref)
}

// toggleRef toggles a specific pack, applying incompatibility resolution,
// persisting the cart, refreshing the panes, and flashing any swap notice.
func (m *ResourcePacksModel) toggleRef(ref RPPackRef) tea.Cmd {
	res := m.sel.Toggle(m.catalog, ref.Category, ref.Pack.Name)
	_ = resourcepacks.SaveSelection(m.inst, m.sel)

	// Clear stale apply status — the cart changed.
	m.applyOK = ""
	m.rebuildCategoryItems()
	m.rebuildPackItems()
	m.rebuildCartItems()

	notice := m.swapNotice(ref, res)
	if notice != "" {
		return m.flashNotice(notice)
	}
	return nil
}

// swapNotice describes the result of a toggle, surfacing any incompatibility
// eviction so the user understands why packs left their cart.
func (m *ResourcePacksModel) swapNotice(ref RPPackRef, res resourcepacks.SwapResult) string {
	label := displayOf(ref.Pack)
	if !res.Enabled {
		return fmt.Sprintf("Removed %s", label)
	}
	if len(res.Removed) > 0 {
		removed := make([]string, 0, len(res.Removed))
		for _, id := range res.Removed {
			if lbl, ok := m.anyPackLabel(id); ok {
				removed = append(removed, lbl)
			} else {
				removed = append(removed, id)
			}
		}
		return fmt.Sprintf("Added %s · swapped out %s", label, joinAnd(removed))
	}
	return fmt.Sprintf("Added %s", label)
}

// anyPackLabel finds a pack's display label anywhere in the catalog.
func (m *ResourcePacksModel) anyPackLabel(name string) (string, bool) {
	for _, cat := range allCategories(m.catalog) {
		for _, p := range cat.Packs {
			if p.Name == name {
				return displayOf(p), true
			}
		}
	}
	return "", false
}
