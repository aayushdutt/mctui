package ui

import (
	"context"
	"errors"
	"strings"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

// ResourcePacksModel is the Vanilla Tweaks resource-pack curation screen:
// left = categories, middle = packs for the selected category, right = cart.
// Build & Apply merges the cart into one resource pack enabled in options.txt.
//
// It works on ANY instance (vanilla or modded) — there is no Fabric gate.
type ResourcePacksModel struct {
	inst *core.Instance
	svc  *resourcepacks.Service

	width         int
	height        int
	compactLayout bool

	// catalogVersion is the Vanilla Tweaks catalog version resolved from the
	// instance MC version (nearest published major.minor).
	catalogVersion string
	// catalogSupported is false when no published catalog maps to this instance.
	catalogSupported bool

	catalog *api.ResourcePackCatalog
	sel     *resourcepacks.Selection

	categories list.Model
	packs      list.Model
	cart       list.Model

	rpFocus rpPanel

	// flat lists derived from the catalog tree for the list models.
	flatCategories []rpCategoryItem
	currentPacks   []RPPackRef

	// expanded tracks which container categories (by path) are open in the tree.
	// Empty = everything collapsed (progressive disclosure default).
	expanded map[string]bool

	rpDialog rpDialogKind

	// searchMode flattens the pack pane to every pack for a screen-wide search;
	// enter on a match jumps to it in its category.
	searchMode bool

	spinner spinner.Model

	// async state
	loading   bool
	loadErr   string
	loadSeq   int
	applying  bool
	applySeq  int
	applyErr  string
	applyOK   string
	statusMsg string
	// noticeToken sequences transient-notice timers so a stale clear can't wipe a
	// fresher message.
	noticeToken int

	loadCancel  context.CancelFunc
	applyCancel context.CancelFunc

	// pane widths derived in SetSize.
	categoriesListW int
	packsListW      int
	cartListW       int
}

// NewResourcePacksModel builds the resource-packs curation screen for inst.
// client is the shared Vanilla Tweaks API client; the model constructs its own
// Service from it (mirroring NewModsModel, which takes *api.ModrinthClient and
// builds a *mods.Service internally).
func NewResourcePacksModel(inst *core.Instance, client *api.VanillaTweaksClient) *ResourcePacksModel {
	// Categories: a tight single-line tree; the header + per-row totals already
	// convey scope, so no status bar. Packs: filterable ("/" search), with a
	// status bar. Cart: status bar, no filter.
	categoriesList := NewThemedList(ThemedListConfig{
		Accent: Active.Primary, AccentSoft: Active.Secondary, SingleLine: true,
	})
	packsList := NewThemedList(ThemedListConfig{
		Accent: Active.Success, AccentSoft: Active.SuccessSoft, StatusBar: true, Filter: true,
	})
	cartList := NewThemedList(ThemedListConfig{
		Accent: Active.Warning, AccentSoft: Active.WarningSoft, StatusBar: true,
	})

	sp := spinner.New()
	sp.Spinner = spinner.Dot
	sp.Style = lipglossSpinnerStyle()

	version, ok := resourcepacks.CatalogVersionFor(inst)

	m := &ResourcePacksModel{
		inst:             inst,
		svc:              resourcepacks.NewService(client),
		catalogVersion:   version,
		catalogSupported: ok,
		catalog:          &api.ResourcePackCatalog{},
		sel:              resourcepacks.NewSelection(version),
		categories:       categoriesList,
		packs:            packsList,
		cart:             cartList,
		rpFocus:          rpPanelCategories,
		spinner:          sp,
		expanded:         map[string]bool{},
	}
	if sel, err := resourcepacks.LoadSelection(inst); err == nil && sel != nil {
		m.sel = sel
	}
	if m.sel.Version == "" {
		m.sel.Version = version
	}
	return m
}

// SetSize updates layout dimensions and re-derives pane sizes.
func (m *ResourcePacksModel) SetSize(w, h int) {
	if w < 1 {
		w = 1
	}
	if h < 1 {
		h = 1
	}
	m.width, m.height = w, h
	m.compactLayout = w < rpSplitMinWidth

	if m.compactLayout {
		listW := max(18, w-6)
		m.categoriesListW = listW
		m.packsListW = listW
		m.cartListW = listW
		m.categories.SetWidth(listW)
		m.packs.SetWidth(listW)
		m.cart.SetWidth(listW)
		m.applyPaneListHeights()
		m.relabelCategories()
		return
	}

	// Three panes: categories (narrow) | packs (wide) | cart (medium).
	catW := max(16, min(26, w*22/100))
	cartW := max(18, min(30, w*26/100))
	// 3 panes × 4 border/padding cells + 2 inter-pane gaps (2 each).
	packW := w - catW - cartW - 12 - 4
	if packW < 24 {
		// Reclaim from the side panes when the middle is starved.
		catW = max(14, w*20/100)
		cartW = max(16, w*22/100)
		packW = w - catW - cartW - 12 - 4
	}
	if packW < 16 {
		packW = 16
	}
	m.categoriesListW = max(10, catW)
	m.packsListW = max(14, packW)
	m.cartListW = max(12, cartW)

	m.categories.SetWidth(m.categoriesListW)
	m.packs.SetWidth(m.packsListW)
	m.cart.SetWidth(m.cartListW)
	m.applyPaneListHeights()
	m.relabelCategories()
}

// relabelCategories rebuilds the visible category rows at the current pane width
// so the right-aligned counts stay aligned after a resize. Cheap no-op until the
// catalog has loaded.
func (m *ResourcePacksModel) relabelCategories() {
	if len(m.catalog.Categories) == 0 {
		return
	}
	m.rebuildCategoryItems()
}

// applyPaneListHeights (re)derives the three list viewport heights from the live
// terminal size AND the current dynamic in-panel chrome: the cart's "Build &
// Apply" CTA block (+2 rows when the cart is non-empty) and the selected
// category's warning line (+1 row when present). Both blocks render outside the
// list bubble, so the budget must reserve room for them or View overflows the
// terminal and clips the footer. Called from SetSize and after every cart/category
// change (which don't trigger a resize) so the budget always matches what View
// renders. Cheap and idempotent.
func (m *ResourcePacksModel) applyPaneListHeights() {
	if m.height < 1 || m.width < 1 {
		return // SetSize hasn't run yet.
	}
	warnReserve := rpCategoryWarningReserve(m.selectedCategoryWarning())
	cartReserve := 0
	if m.sel.Count() > 0 {
		cartReserve = rpCartCTAReserve
	}

	if m.compactLayout {
		// Stacked: the CTA and warning add to total height directly, so fold them
		// into the shared budget before distributing to the three panes.
		catH, packH, cartH := rpCompactListHeights(m.height, m.width, warnReserve+cartReserve)
		m.categories.SetHeight(catH)
		m.packs.SetHeight(packH)
		m.cart.SetHeight(cartH)
		return
	}

	// Split: all three panes share one budget and are joined horizontally, so the
	// row height is the tallest pane. Shrink the pack and cart lists by their own
	// extra chrome; categories keeps the full budget.
	listH := rpListViewportHeight(m.height, m.width)
	m.categories.SetHeight(listH)
	m.packs.SetHeight(max(1, listH-warnReserve))
	m.cart.SetHeight(max(1, listH-cartReserve))
}

// selectedCategoryWarning returns the warning text for the currently highlighted
// category, or "" when there is none.
func (m *ResourcePacksModel) selectedCategoryWarning() string {
	cat, ok := m.selectedCategory()
	if !ok || cat.category.Warning == nil {
		return ""
	}
	return strings.TrimSpace(cat.category.Warning.Text)
}

// Init implements tea.Model. It kicks off the catalog fetch.
func (m *ResourcePacksModel) Init() tea.Cmd {
	if !m.catalogSupported {
		return nil
	}
	return tea.Batch(m.spinner.Tick, m.loadCatalogCmd())
}

// CancelPending stops any in-flight catalog fetch and build/apply. Call before
// discarding the model (e.g. leaving the screen).
func (m *ResourcePacksModel) CancelPending() {
	if m.loadCancel != nil {
		m.loadCancel()
		m.loadCancel = nil
	}
	if m.applyCancel != nil {
		m.applyCancel()
		m.applyCancel = nil
	}
}

// loadCatalogCmd fetches the catalog for the instance version off the event loop.
func (m *ResourcePacksModel) loadCatalogCmd() tea.Cmd {
	if m.loadCancel != nil {
		m.loadCancel()
		m.loadCancel = nil
	}
	m.loading = true
	m.loadErr = ""
	m.loadSeq++
	seq := m.loadSeq
	svc := m.svc
	inst := m.inst
	version := m.catalogVersion
	ctx, cancel := context.WithCancel(context.Background())
	m.loadCancel = cancel
	return func() tea.Msg {
		cat, err := svc.FetchCatalog(ctx, inst)
		if err != nil && errors.Is(err, context.Canceled) {
			return rpCatalogLoadedMsg{seq: seq, version: version, err: context.Canceled}
		}
		return rpCatalogLoadedMsg{catalog: cat, version: version, err: err, seq: seq}
	}
}

// applyCmd builds the merged pack and applies it (writes zip + enables in
// options.txt) off the event loop, reporting via rpApplyDoneMsg.
func (m *ResourcePacksModel) applyCmd() tea.Cmd {
	if m.applyCancel != nil {
		m.applyCancel()
		m.applyCancel = nil
	}
	m.applying = true
	m.applyErr = ""
	m.applyOK = ""
	svc := m.svc
	inst := m.inst
	// Hand the goroutine its OWN deep copy of the selection. BuildAndApply mutates
	// (MarkApplied) and persists the cart; mutating the live m.sel here would race
	// View()/headerBlock(), which read m.sel concurrently while m.applying is true.
	// The main thread re-adopts the persisted state from disk in handleApplyDone.
	sel := m.sel.Clone()
	m.applySeq++
	seq := m.applySeq
	ctx, cancel := context.WithCancel(context.Background())
	m.applyCancel = cancel
	ch := make(chan rpApplyDoneMsg, 1)
	go func() {
		defer cancel()
		res, err := svc.BuildAndApply(ctx, inst, sel)
		ch <- rpApplyDoneMsg{result: res, err: err, seq: seq}
	}()
	return func() tea.Msg {
		return <-ch
	}
}

// instanceContextLine builds the header subtitle: instance, MC/catalog version,
// cart size, and applied/dirty state.
func (m *ResourcePacksModel) instanceContextLine() string {
	if m.inst == nil {
		return ""
	}
	var b strings.Builder
	b.WriteString(m.inst.Name)
	b.WriteString(" · Minecraft ")
	b.WriteString(m.inst.Version)
	if m.catalogSupported && m.catalogVersion != "" && m.catalogVersion != m.inst.Version {
		b.WriteString(" · catalog ")
		b.WriteString(m.catalogVersion)
	}
	return b.String()
}
