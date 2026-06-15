package ui

import (
	"github.com/aayushdutt/mctui/internal/api"
	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
)

// This file owns global pack search (flatten → filter → jump-into-context) and
// the preview/video browser-open actions, which both operate on the highlighted
// pack across categories.

// openHighlightedPreview/openHighlightedVideo act on the focused-pane selection.
func (m *ResourcePacksModel) openHighlightedPreview() tea.Cmd {
	if ref, ok := m.selectedPackRef(); ok {
		return m.openPreviewFor(ref)
	}
	return nil
}

func (m *ResourcePacksModel) openHighlightedVideo() tea.Cmd {
	if ref, ok := m.selectedPackRef(); ok {
		return m.openVideoFor(ref)
	}
	return nil
}

// openPreviewFor opens the pack's preview image (animated when available, else
// the static icon) in the browser.
func (m *ResourcePacksModel) openPreviewFor(ref RPPackRef) tea.Cmd {
	url := api.ResourcePackPreviewURL(m.catalogVersion, ref.Pack)
	if !isHTTPURL(url) {
		return m.flashNotice("No preview available")
	}
	if err := openURL(url); err != nil {
		return m.flashNotice("Couldn't open preview")
	}
	return m.flashNotice("Opened preview for " + displayOf(ref.Pack))
}

// openVideoFor opens the pack's video/credit link, if it has one.
func (m *ResourcePacksModel) openVideoFor(ref RPPackRef) tea.Cmd {
	if !isHTTPURL(ref.Pack.Video) {
		return m.flashNotice("No video for this pack")
	}
	if err := openURL(ref.Pack.Video); err != nil {
		return m.flashNotice("Couldn't open video")
	}
	return m.flashNotice("Opened video for " + displayOf(ref.Pack))
}

// enterSearch flattens every pack into the pack pane and focuses it for filtering.
func (m *ResourcePacksModel) enterSearch() {
	m.searchMode = true
	m.rpFocus = rpPanelPacks
	m.rebuildPackItems()
}

// exitSearch leaves search mode, clearing the filter and restoring the
// per-category pack view. No-op when not searching.
func (m *ResourcePacksModel) exitSearch() {
	if !m.searchMode {
		return
	}
	m.searchMode = false
	if m.packs.FilterState() != list.Unfiltered {
		m.packs.ResetFilter()
	}
	m.rebuildPackItems()
}

// jumpToSearchResult exits search and lands on the highlighted pack inside its
// category (expanding ancestors), where it can be toggled/inspected in context.
func (m *ResourcePacksModel) jumpToSearchResult() {
	it, ok := m.packs.SelectedItem().(rpPackItem)
	m.searchMode = false
	if m.packs.FilterState() != list.Unfiltered {
		m.packs.ResetFilter()
	}
	if !ok {
		m.rebuildPackItems()
		return
	}
	ref := it.pack
	m.expandAncestorsFor(ref.Category)
	m.rebuildCategoryItems()
	m.selectCategoryByDisplay(ref.Category)
	m.rebuildPackItems()
	m.selectPackByName(ref.Pack.Name)
	m.rpFocus = rpPanelPacks
}

// expandAncestorsFor opens every container on the path to the category with the
// given display name, so its row is visible after a jump.
func (m *ResourcePacksModel) expandAncestorsFor(category string) {
	if m.expanded == nil {
		m.expanded = map[string]bool{}
	}
	var find func(c api.RPCategory, parent string, ancestors []string) bool
	find = func(c api.RPCategory, parent string, ancestors []string) bool {
		path := joinPath(parent, c.Category)
		if c.Category == category {
			for _, a := range ancestors {
				m.expanded[a] = true
			}
			return true
		}
		next := append(append([]string{}, ancestors...), path)
		for _, sub := range c.Categories {
			if find(sub, path, next) {
				return true
			}
		}
		return false
	}
	for _, c := range m.catalog.Categories {
		if find(c, "", nil) {
			return
		}
	}
}

// selectCategoryByDisplay highlights the visible category row with the given
// display name (first match).
func (m *ResourcePacksModel) selectCategoryByDisplay(category string) {
	for i, it := range m.flatCategories {
		if it.category.Category == category {
			m.categories.Select(i)
			return
		}
	}
}

// selectPackByName highlights the pack row with the given id, if present.
func (m *ResourcePacksModel) selectPackByName(name string) {
	for i, it := range m.packs.Items() {
		if p, ok := it.(rpPackItem); ok && p.pack.Pack.Name == name {
			m.packs.Select(i)
			return
		}
	}
}
