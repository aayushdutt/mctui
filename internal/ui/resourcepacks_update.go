package ui

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

// This file holds the Bubbletea event loop for the resource-packs screen: the
// Update dispatch, async message handlers, key routing, and pane-focus plumbing.
// The category tree, cart/selection bridge, and global search live in their own
// resourcepacks_{tree,cart,search}.go files.

// Update implements tea.Model. The primary handler lives here (mirroring the
// mods screen, where the entry point is in mods_update.go). Focus/navigation,
// toggle, and async message handling all flow through this switch.
func (m *ResourcePacksModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case spinner.TickMsg:
		if !m.loading && !m.applying {
			return m, nil
		}
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd

	case rpCatalogLoadedMsg:
		return m.handleCatalogLoaded(msg)

	case rpApplyDoneMsg:
		return m.handleApplyDone(msg)

	case rpClearNoticeMsg:
		if msg.token == m.noticeToken {
			m.statusMsg = ""
		}
		return m, nil

	case tea.MouseMsg:
		return m.handleMouse(msg)

	case tea.KeyMsg:
		return m.handleKey(msg)
	}

	// Forward any other message to the focused list.
	return m, m.forwardToFocused(msg)
}

func (m *ResourcePacksModel) handleCatalogLoaded(msg rpCatalogLoadedMsg) (tea.Model, tea.Cmd) {
	if msg.seq != m.loadSeq {
		return m, nil // stale fetch
	}
	m.loading = false
	m.loadCancel = nil
	if msg.err != nil {
		if errors.Is(msg.err, context.Canceled) {
			return m, nil
		}
		m.loadErr = msg.err.Error()
		return m, nil
	}
	m.loadErr = ""
	if msg.catalog == nil {
		m.catalog = &api.ResourcePackCatalog{}
	} else {
		m.catalog = msg.catalog
	}
	if msg.catalog != nil && msg.catalog.Version != "" {
		m.catalogVersion = msg.catalog.Version
	} else if msg.version != "" {
		m.catalogVersion = msg.version
	}
	// Open containers that already hold cart picks so the user's choices are
	// visible on open; everything else stays collapsed (progressive disclosure).
	m.autoExpandSelected()
	m.rebuildCategoryItems()
	m.rebuildPackItems()
	m.rebuildCartItems()
	return m, nil
}

func (m *ResourcePacksModel) handleApplyDone(msg rpApplyDoneMsg) (tea.Model, tea.Cmd) {
	if msg.seq != m.applySeq {
		return m, nil // stale apply (cancelled or superseded)
	}
	m.applying = false
	m.applyCancel = nil
	if msg.err != nil {
		if errors.Is(msg.err, context.Canceled) {
			return m, nil
		}
		m.applyErr = msg.err.Error()
		return m, nil
	}
	m.applyErr = ""
	// BuildAndApply snapshots the cart as applied; reload from disk to stay in
	// sync, then refresh the dirty markers.
	if sel, err := resourcepacks.LoadSelection(m.inst); err == nil && sel != nil {
		m.sel = sel
	}
	n := 0
	if msg.result != nil {
		n = msg.result.PackCount
	}
	m.applyOK = fmt.Sprintf("Applied %d tweak%s", n, plural(n))
	m.rebuildCategoryItems()
	m.rebuildPackItems()
	m.rebuildCartItems()
	return m, nil
}

func (m *ResourcePacksModel) handleMouse(msg tea.MouseMsg) (tea.Model, tea.Cmd) {
	if m.applying || m.loading {
		return m, nil
	}
	if m.rpDialog != rpDialogNone {
		return m, nil
	}
	switch msg.Type {
	case tea.MouseWheelUp:
		m.focusedList().CursorUp()
		m.afterFocusedListMove()
	case tea.MouseWheelDown:
		m.focusedList().CursorDown()
		m.afterFocusedListMove()
	}
	return m, nil
}

func (m *ResourcePacksModel) handleKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	// Confirm-clear dialog owns all keys while open.
	if m.rpDialog == rpDialogConfirmClear {
		return m.handleClearDialogKey(msg)
	}

	// While building, only esc (cancel) is honored.
	if m.applying {
		if msg.String() == "esc" {
			m.CancelPending()
			m.applying = false
			return m, func() tea.Msg { return NavigateToHome{} }
		}
		return m, nil
	}

	// If the pack list is actively filtering, let it consume keys (except esc,
	// which the list itself uses to exit the filter).
	if m.rpFocus == rpPanelPacks && m.packs.FilterState() == list.Filtering {
		var cmd tea.Cmd
		m.packs, cmd = m.packs.Update(msg)
		// esc cancels the filter mid-type; finish leaving search so the pane
		// rebuilds to the per-category view instead of a stuck flattened list.
		if m.searchMode && m.packs.FilterState() == list.Unfiltered {
			m.exitSearch()
		}
		return m, cmd
	}

	// Global search (after the filter is applied): esc leaves search; enter/space
	// jump to the highlighted pack inside its category to toggle/inspect it there.
	if m.searchMode && m.rpFocus == rpPanelPacks {
		switch msg.String() {
		case "esc":
			m.exitSearch()
			return m, nil
		case "enter", " ":
			m.jumpToSearchResult()
			return m, nil
		}
	}

	switch msg.String() {
	case "esc":
		m.CancelPending()
		return m, func() tea.Msg { return NavigateToHome{} }

	case "tab":
		m.cyclePane(1)
		return m, nil
	case "shift+tab":
		m.cyclePane(-1)
		return m, nil
	case "left", "h":
		if m.rpFocus == rpPanelPacks && m.packs.FilterState() == list.Filtering {
			break
		}
		m.navLeft()
		return m, nil
	case "right", "l":
		if m.rpFocus == rpPanelPacks && m.packs.FilterState() == list.Filtering {
			break
		}
		m.navRight()
		return m, nil

	case "/":
		// Global search: flatten every pack, then start the list filter.
		m.enterSearch()
		var cmd tea.Cmd
		m.packs, cmd = m.packs.Update(msg)
		return m, cmd

	case " ", "enter":
		// Categories: expand/dive. Packs/cart: toggle the pack.
		return m, m.activate()

	case "o", "O":
		return m, m.openHighlightedPreview()

	case "w", "W":
		return m, m.openHighlightedVideo()

	case "b", "B":
		return m, m.startApply()

	case "c", "C":
		if m.sel.Count() > 0 {
			m.rpDialog = rpDialogConfirmClear
		}
		return m, nil

	case "up", "k", "down", "j", "pgup", "pgdown", "home", "end":
		var cmd tea.Cmd
		l := m.focusedList()
		*l, cmd = l.Update(msg)
		m.afterFocusedListMove()
		return m, cmd
	}

	// Default: forward to the focused list (covers filter typing, etc).
	return m, m.forwardToFocused(msg)
}

func (m *ResourcePacksModel) handleClearDialogKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "y", "Y", "enter":
		m.rpDialog = rpDialogNone
		m.sel.Clear()
		_ = resourcepacks.SaveSelection(m.inst, m.sel)
		m.applyOK = ""
		m.applyErr = ""
		m.rebuildCategoryItems()
		m.rebuildPackItems()
		m.rebuildCartItems()
		return m, m.flashNotice("Cleared your pack")
	case "n", "N", "esc", "backspace":
		m.rpDialog = rpDialogNone
		return m, nil
	}
	return m, nil
}

// startApply kicks off Build & Apply if the cart is non-empty.
func (m *ResourcePacksModel) startApply() tea.Cmd {
	if m.sel.Count() == 0 {
		return m.flashNotice("Your pack is empty — add some tweaks first")
	}
	m.applyOK = ""
	m.applyErr = ""
	return tea.Batch(m.spinner.Tick, m.applyCmd())
}

// cyclePane moves focus by dir (+1 forward, -1 back) across the three panes.
func (m *ResourcePacksModel) cyclePane(dir int) {
	// Exit any active search/filter when leaving the packs pane.
	m.exitSearch()
	if m.packs.FilterState() != list.Unfiltered {
		m.packs.ResetFilter()
	}
	n := 3
	m.rpFocus = rpPanel((int(m.rpFocus) + dir + n) % n)
	if m.rpFocus == rpPanelPacks {
		m.rebuildPackItems()
	}
}

// focusedList returns a pointer to the list model for the focused pane.
func (m *ResourcePacksModel) focusedList() *list.Model {
	switch m.rpFocus {
	case rpPanelPacks:
		return &m.packs
	case rpPanelCart:
		return &m.cart
	default:
		return &m.categories
	}
}

// afterFocusedListMove reacts to cursor movement: changing the highlighted
// category repopulates the pack pane.
func (m *ResourcePacksModel) afterFocusedListMove() {
	if m.rpFocus == rpPanelCategories {
		m.rebuildPackItems()
	}
}

// forwardToFocused passes msg to the focused list and reacts to category moves.
func (m *ResourcePacksModel) forwardToFocused(msg tea.Msg) tea.Cmd {
	var cmd tea.Cmd
	l := m.focusedList()
	*l, cmd = l.Update(msg)
	m.afterFocusedListMove()
	return cmd
}

// flashNotice sets a transient status line and schedules its removal.
func (m *ResourcePacksModel) flashNotice(text string) tea.Cmd {
	m.statusMsg = text
	m.noticeToken++
	token := m.noticeToken
	return tea.Tick(4*time.Second, func(time.Time) tea.Msg {
		return rpClearNoticeMsg{token: token}
	})
}
