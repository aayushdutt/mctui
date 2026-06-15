package ui

import (
	"strings"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
)

// This file owns the collapsible category tree: building the visible rows from
// the catalog (progressive disclosure), tree-aware navigation (expand/collapse/
// drill), and the path/count helpers the rows are keyed by.

// rpPathSep joins category display names into a stable tree path. NUL can't
// appear in a display name, so it's a safe separator.
const rpPathSep = "\x00"

// rebuildCategoryItems rebuilds the visible category rows from the catalog tree,
// emitting children only under expanded containers (progressive disclosure) and
// preserving the highlighted row by path across expand/collapse.
func (m *ResourcePacksModel) rebuildCategoryItems() {
	prevPath := m.selectedCategoryPath()
	prevIdx := m.categories.Index()

	m.flatCategories = m.flatCategories[:0]
	for _, c := range m.catalog.Categories {
		m.appendCategoryRow(c, 0, "", "")
	}
	items := make([]list.Item, 0, len(m.flatCategories))
	for _, ci := range m.flatCategories {
		items = append(items, ci)
	}
	m.categories.SetItems(items)

	// Restore the highlight by path; fall back to a clamped index when the path
	// no longer exists (e.g. a child that just collapsed).
	sel := -1
	if prevPath != "" {
		for i, it := range m.flatCategories {
			if it.path == prevPath {
				sel = i
				break
			}
		}
	}
	if sel < 0 {
		sel = prevIdx
	}
	if sel >= len(items) {
		sel = len(items) - 1
	}
	if sel < 0 {
		sel = 0
	}
	if len(items) > 0 {
		m.categories.Select(sel)
	}
}

// appendCategoryRow walks the tree depth-first, emitting one row per node and
// descending into expanded containers only. Counts aggregate over the subtree.
func (m *ResourcePacksModel) appendCategoryRow(c api.RPCategory, depth int, parentPath, parentCrumb string) {
	path := joinPath(parentPath, c.Category)
	crumb := joinCrumb(parentCrumb, c.Category)
	container := len(c.Categories) > 0
	expanded := container && m.expanded[path]

	m.flatCategories = append(m.flatCategories, rpCategoryItem{
		category:      c,
		path:          path,
		crumb:         crumb,
		depth:         depth,
		isContainer:   container,
		expanded:      expanded,
		totalPacks:    totalPacksIn(c),
		selectedCount: m.selectedInSubtree(c),
		rowWidth:      m.categoriesListW,
	})
	if expanded {
		for _, sub := range c.Categories {
			m.appendCategoryRow(sub, depth+1, path, crumb)
		}
	}
}

// navRight drills further in: expand a collapsed container, dive a pure container
// to its first child, otherwise move focus one pane to the right.
func (m *ResourcePacksModel) navRight() {
	if m.rpFocus == rpPanelCategories {
		if it, ok := m.selectedCategory(); ok && it.isContainer {
			if !it.expanded {
				m.setExpanded(it.path, true)
				return
			}
			if len(it.category.Packs) == 0 {
				// No packs of its own — step into the subcategories instead of
				// focusing an empty pack pane.
				m.selectFirstChild(it.path)
				return
			}
		}
	}
	m.focusPane(1)
}

// navLeft drills back out: collapse an expanded container, hop a child to its
// parent, otherwise move focus one pane to the left.
func (m *ResourcePacksModel) navLeft() {
	if m.rpFocus == rpPanelCategories {
		if it, ok := m.selectedCategory(); ok {
			if it.isContainer && it.expanded {
				m.setExpanded(it.path, false)
				return
			}
			if it.depth > 0 {
				m.selectCategoryByPath(parentPath(it.path))
				m.rebuildPackItems()
				return
			}
		}
		return // top-level leaf/collapsed: nothing further left
	}
	m.focusPane(-1)
}

// focusPane moves focus by dir without wrapping (categories ↔ packs ↔ cart),
// resetting any pack filter and repopulating packs when entering that pane.
func (m *ResourcePacksModel) focusPane(dir int) {
	m.exitSearch()
	if m.packs.FilterState() != list.Unfiltered {
		m.packs.ResetFilter()
	}
	next := int(m.rpFocus) + dir
	if next < int(rpPanelCategories) || next > int(rpPanelCart) {
		return
	}
	m.rpFocus = rpPanel(next)
	if m.rpFocus == rpPanelPacks {
		m.rebuildPackItems()
	}
}

// activate handles enter/space: on a container, toggle expand/collapse; on a leaf
// category, dive into its packs; on a pack/cart row, toggle cart membership.
func (m *ResourcePacksModel) activate() tea.Cmd {
	if m.rpFocus != rpPanelCategories {
		return m.toggleSelectedPack()
	}
	if it, ok := m.selectedCategory(); ok {
		if it.isContainer {
			m.setExpanded(it.path, !it.expanded)
			return nil
		}
		m.focusPane(1) // leaf: dive into its packs
	}
	return nil
}

// setExpanded opens/closes a container and keeps the highlight on it.
func (m *ResourcePacksModel) setExpanded(path string, open bool) {
	if m.expanded == nil {
		m.expanded = map[string]bool{}
	}
	m.expanded[path] = open
	m.rebuildCategoryItems()
	m.selectCategoryByPath(path)
	m.rebuildPackItems()
}

// selectCategoryByPath highlights the row with the given path, if visible.
func (m *ResourcePacksModel) selectCategoryByPath(path string) bool {
	for i, it := range m.flatCategories {
		if it.path == path {
			m.categories.Select(i)
			return true
		}
	}
	return false
}

// selectFirstChild moves the highlight to the first child row of a container.
func (m *ResourcePacksModel) selectFirstChild(path string) {
	for i, it := range m.flatCategories {
		if it.path != path {
			continue
		}
		if i+1 < len(m.flatCategories) && m.flatCategories[i+1].depth > it.depth {
			m.categories.Select(i + 1)
			m.rebuildPackItems()
		}
		return
	}
}

// selectedCategoryPath returns the path of the highlighted category row, or "".
func (m *ResourcePacksModel) selectedCategoryPath() string {
	if it, ok := m.selectedCategory(); ok {
		return it.path
	}
	return ""
}

// selectedCategory returns the currently highlighted category, or false.
func (m *ResourcePacksModel) selectedCategory() (rpCategoryItem, bool) {
	it, ok := m.categories.SelectedItem().(rpCategoryItem)
	return it, ok
}

// autoExpandSelected opens every container whose subtree holds a selected pack.
func (m *ResourcePacksModel) autoExpandSelected() {
	if m.expanded == nil {
		m.expanded = map[string]bool{}
	}
	var walk func(c api.RPCategory, parent string)
	walk = func(c api.RPCategory, parent string) {
		path := joinPath(parent, c.Category)
		if len(c.Categories) > 0 && m.selectedInSubtree(c) > 0 {
			m.expanded[path] = true
		}
		for _, sub := range c.Categories {
			walk(sub, path)
		}
	}
	for _, c := range m.catalog.Categories {
		walk(c, "")
	}
}

// joinPath builds a NUL-separated tree path from a parent path and a node name.
func joinPath(parent, name string) string {
	if parent == "" {
		return name
	}
	return parent + rpPathSep + name
}

// joinCrumb builds a human breadcrumb ("Aesthetic › More Zombies").
func joinCrumb(parent, name string) string {
	if parent == "" {
		return name
	}
	return parent + " › " + name
}

// parentPath returns the path with its last segment removed, or "" at the root.
func parentPath(path string) string {
	if i := strings.LastIndex(path, rpPathSep); i >= 0 {
		return path[:i]
	}
	return ""
}

// totalPacksIn counts packs in a category subtree (self + all descendants).
func totalPacksIn(c api.RPCategory) int {
	n := len(c.Packs)
	for _, sub := range c.Categories {
		n += totalPacksIn(sub)
	}
	return n
}

// selectedInSubtree counts cart packs across a category subtree.
func (m *ResourcePacksModel) selectedInSubtree(c api.RPCategory) int {
	n := m.sel.CountInCategory(c.Category)
	for _, sub := range c.Categories {
		n += m.selectedInSubtree(sub)
	}
	return n
}
