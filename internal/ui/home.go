// Package ui contains all TUI view components.
// Each view is a Bubbletea model that can be composed into the main app.
package ui

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/loader"
	"github.com/aayushdutt/mctui/internal/mods"
	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// HomeModel is the main instance list view
type HomeModel struct {
	list      list.Model
	instances []*core.Instance
	width     int
	height    int
	keys      homeKeyMap
	loading   bool
	accounts  *core.AccountManager

	// Two-column layout state, derived in SetSize at the homeSplitMinWidth
	// breakpoint (mirrors mods_view.go / resourcepacks_view.go).
	compactLayout bool // true below the breakpoint: single column + summary line
	listPaneW     int  // outer width of the list panel (border included)
	detailPaneW   int  // outer width of the detail panel (border included)

	// modCountCache memoizes the installed .jar count per instance ID. Populated
	// lazily on first view of a selection (mods.ListInstalledJars does real but
	// cheap I/O) and invalidated in SetInstances. NEVER computed in Update.
	modCountCache map[string]int

	// activeTab is the selected detail facet, stepped with ←/→ (and h/l, [ /]) in Update.
	activeTab homeTab

	// paneFocus tracks which column owns arrow keys / Enter. focusList (default)
	// drives the instance list; focusDetail drives the active tab's rows. Tab and
	// →/l move focus to the detail pane; Tab again or ←/h/Esc returns to the list.
	paneFocus paneFocus

	// detailCursor indexes the focused detail tab's rows (including the leading
	// "+ Add…" row on Mods / Resource packs). Reset to 0 when the active tab or
	// the selected instance changes.
	detailCursor int

	// lastSelID tracks the selected instance ID across Updates so the detail
	// cursor + focus can be reset when the list cursor moves to a new instance.
	lastSelID string

	// facetCache memoizes the disk-backed facet readers (worlds, resource packs) per
	// instance ID, populated lazily on first view of a (instance, tab) and
	// invalidated wholesale in SetInstances. NEVER read in Update's key path.
	facetCache map[string]*facetData

	// Microsoft session hint from background check (MSA accounts only)
	sessionRemote sessionRemoteLine

	// transientBanner is a one-line notice (e.g. session gate network error)
	transientBanner string

	// Delete confirmation state
	confirmDelete  bool
	deleteTarget   *core.Instance
	deleteFocusYes bool // which option arrows / Enter apply to (default Yes so Enter still confirms delete)

	// Actions popup state (home_actions.go). Mutually exclusive with the other
	// overlays — never armed while confirmDelete or palette is active.
	actionsPopup  bool
	actionsTarget *core.Instance
	actionsCursor int

	// Command-palette state (home_palette.go). Mutually exclusive with the other
	// overlays. The textinput owns keystrokes while palette is true.
	palette        bool
	paletteInput   textinput.Model
	paletteEntries []paletteEntry
	paletteCursor  int
}

// paneFocus is which of the two home columns owns arrow/Enter input.
type paneFocus int

const (
	focusList   paneFocus = iota // arrows drive the instance list (default)
	focusDetail                  // arrows drive the active tab's rows
)

// homeTab enumerates the detail facets shown in the right panel. The order here
// is the left-to-right step order for ←/→ (and h/l, [ /]).
type homeTab int

const (
	tabOverview homeTab = iota
	tabMods
	tabResourcePacks
	homeTabCount
)

// homeTabTitles labels each facet for the tab strip, in cycle order.
var homeTabTitles = [homeTabCount]string{
	tabOverview:      "Overview",
	tabMods:          "Mods",
	tabResourcePacks: "Resource packs",
}

// homeTabIsList reports whether a tab renders a cursor-navigable list of rows
// (Mods / Resource packs). Overview has no rows, so detail-focus arrows are
// no-ops there.
func homeTabIsList(t homeTab) bool {
	switch t {
	case tabMods, tabResourcePacks:
		return true
	default:
		return false
	}
}

// facetData caches the disk-backed facet readers for one instance. Fields are
// populated lazily and independently the first time a given facet is viewed; a
// loaded bool guards the empty-vs-not-yet-read distinction.
type facetData struct {
	worlds       []worldEntry
	worldsLoaded bool
	packs        []resourcePackEntry
	packsLoaded  bool
}

type sessionRemoteLine int

const (
	sessionRemoteUnset sessionRemoteLine = iota
	sessionRemoteChecking
	sessionRemoteInvalid
	sessionRemoteUncertain
)

type homeKeyMap struct {
	Launch      key.Binding
	PlayOffline key.Binding
	NewInst     key.Binding
	Mods        key.Binding
	ResPacks    key.Binding
	Settings    key.Binding
	Delete      key.Binding
	Auth        key.Binding
	OpenFolder  key.Binding
}

func defaultHomeKeyMap() homeKeyMap {
	return homeKeyMap{
		Launch: key.NewBinding(
			// `l` is now "focus the detail pane" (vim-right), handled before this
			// binding — so Launch is Enter only.
			key.WithKeys("enter"),
			key.WithHelp("enter", "launch"),
		),
		PlayOffline: key.NewBinding(
			key.WithKeys("o"),
			key.WithHelp("o", "play offline"),
		),
		NewInst: key.NewBinding(
			key.WithKeys("n"),
			key.WithHelp("n", "new"),
		),
		Mods: key.NewBinding(
			key.WithKeys("m"),
			key.WithHelp("m", "mods"),
		),
		ResPacks: key.NewBinding(
			key.WithKeys("p"),
			key.WithHelp("p", "resource packs"),
		),
		Settings: key.NewBinding(
			key.WithKeys("s"),
			key.WithHelp("s", "settings"),
		),
		Delete: key.NewBinding(
			key.WithKeys("d"),
			key.WithHelp("d", "delete"),
		),
		Auth: key.NewBinding(
			key.WithKeys("a"),
			key.WithHelp("a", "accounts"),
		),
		OpenFolder: key.NewBinding(
			key.WithKeys("f"),
			key.WithHelp("f", "open folder"),
		),
	}
}

// instanceItem represents a Minecraft instance in the list
type instanceItem struct {
	instance *core.Instance
}

func (i instanceItem) Title() string { return i.instance.Name }

func (i instanceItem) Description() string {
	loader := i.instance.Loader
	if loader == "" || loader == "vanilla" {
		loader = "Vanilla"
	}

	lastPlayed := "Never played"
	if !i.instance.LastPlayed.IsZero() {
		lastPlayed = formatRelativeTime(i.instance.LastPlayed)
	}

	return fmt.Sprintf("%s • %s • %s", i.instance.Version, loader, lastPlayed)
}
func (i instanceItem) FilterValue() string { return i.instance.Name }

// addInstanceItem is the trailing action row in the instance list: a shared
// "+ New instance" ActionButton (rendered by homeInstanceDelegate). It is not a
// real instance — selecting it and pressing Enter opens the new-instance wizard
// (see Update). Its empty FilterValue keeps it out of search results.
type addInstanceItem struct{}

func (addInstanceItem) FilterValue() string { return "" }

func formatRelativeTime(t time.Time) string {
	diff := time.Since(t)
	switch {
	case diff < time.Minute:
		return "Just now"
	case diff < time.Hour:
		return fmt.Sprintf("%dm ago", int(diff.Minutes()))
	case diff < 24*time.Hour:
		return fmt.Sprintf("%dh ago", int(diff.Hours()))
	case diff < 7*24*time.Hour:
		return fmt.Sprintf("%dd ago", int(diff.Hours()/24))
	default:
		return t.Format("Jan 2")
	}
}

// NewHomeModel creates a new home view model
func NewHomeModel() *HomeModel {
	base := ThemeListDelegate(Active.Primary, Active.Secondary)

	l := list.New([]list.Item{}, &homeInstanceDelegate{DefaultDelegate: base}, 0, 0)
	// The screen title now lives in the StatusBar row above the columns and the
	// "Instances" label sits in the list panel border, so the list's internal
	// title bar is turned off. The status bar is off too: the title row already
	// shows an accurate "N instances" count, and the built-in bar would otherwise
	// miscount the synthetic trailing "+ New instance" row as an item.
	l.SetShowStatusBar(false)
	l.SetFilteringEnabled(true)
	l.SetShowTitle(false)
	l.SetShowHelp(false)
	ThemeListChrome(&l)

	return &HomeModel{
		list:          l,
		keys:          defaultHomeKeyMap(),
		loading:       true,
		modCountCache: map[string]int{},
		facetCache:    map[string]*facetData{},
	}
}

// ApplyTheme re-dresses the (long-lived) home list from the active theme. The
// app calls this after the theme changes so the home screen — which, unlike the
// per-entry screens, is not rebuilt on navigation — picks up the new palette.
func (m *HomeModel) ApplyTheme() {
	base := ThemeListDelegate(Active.Primary, Active.Secondary)
	m.list.SetDelegate(&homeInstanceDelegate{DefaultDelegate: base})
	ThemeListChrome(&m.list)
}

// SetInstances updates the instance list. If selectID is non-empty, the cursor moves to that instance (usually index 0 for a new instance).
func (m *HomeModel) SetInstances(instances []*core.Instance, selectID string) {
	m.instances = instances
	m.loading = false
	// Invalidate the lazy mod-count cache — instances (and their mod folders) may
	// have changed.
	m.modCountCache = map[string]int{}
	// Invalidate the lazy facet cache (worlds/resource packs) for the same reason.
	m.facetCache = map[string]*facetData{}

	// Sort by max(LastPlayed, CreatedAt) — newest first
	sort.Slice(instances, func(i, j int) bool {
		return core.RecencyForSort(instances[i]).After(core.RecencyForSort(instances[j]))
	})

	items := make([]list.Item, 0, len(instances)+1)
	for _, inst := range instances {
		items = append(items, instanceItem{instance: inst})
	}
	// Trailing "+ New instance" action row, always last.
	items = append(items, addInstanceItem{})
	m.list.SetItems(items)

	if selectID != "" {
		for i, inst := range instances {
			if inst.ID == selectID {
				m.list.Select(i)
				m.resetDetailFocus()
				return
			}
		}
		m.list.Select(0)
	}
	m.resetDetailFocus()
}

// resetDetailFocus returns to the list pane, clears the detail cursor, and
// re-syncs lastSelID to the current selection. Called whenever the selected
// instance changes (list navigation, SetInstances) so stale cursors/focus never
// linger.
func (m *HomeModel) resetDetailFocus() {
	m.focusListPane()
	if inst := m.SelectedInstance(); inst != nil {
		m.lastSelID = inst.ID
	} else {
		m.lastSelID = ""
	}
}

func (m *HomeModel) SetAccountManager(am *core.AccountManager) {
	m.accounts = am
}

// SetSessionCheckStarted marks the UI as verifying the active Microsoft session.
func (m *HomeModel) SetSessionCheckStarted() {
	if acc := m.activeMSAAccount(); acc != nil {
		m.sessionRemote = sessionRemoteChecking
	}
}

// ApplyActiveSessionCheckResult updates session status from a background check.
func (m *HomeModel) ApplyActiveSessionCheckResult(res ActiveSessionCheckResult) {
	switch res.Status {
	case ActiveSessionNotApplicable:
		m.sessionRemote = sessionRemoteUnset
	case ActiveSessionOK:
		m.sessionRemote = sessionRemoteUnset
	case ActiveSessionInvalid:
		m.sessionRemote = sessionRemoteInvalid
	case ActiveSessionUncertain:
		m.sessionRemote = sessionRemoteUncertain
	}
}

// SetTransientBanner shows a short notice above the footer; cleared on the next key press.
func (m *HomeModel) SetTransientBanner(s string) {
	m.transientBanner = s
	m.applyListSize()
}

func (m *HomeModel) activeMSAAccount() *core.Account {
	if m.accounts == nil {
		return nil
	}
	acc := m.accounts.GetActive()
	if acc == nil || acc.Type != core.AccountTypeMSA {
		return nil
	}
	return acc
}

// noOnlineSession reports whether launching would route through sign-in rather
// than starting the game directly (no account, or an MSA session that needs
// re-auth). Shared by buildFooter's launch label and the detail panel's action
// line so they stay consistent.
func (m *HomeModel) noOnlineSession() bool {
	if m.accounts == nil || m.accounts.GetActive() == nil {
		return true
	}
	if acc := m.activeMSAAccount(); acc != nil {
		switch m.sessionRemote {
		case sessionRemoteInvalid, sessionRemoteUncertain:
			return true
		}
	}
	return false
}

// launchSelected returns the command to launch the currently selected instance.
// Offline goes straight to the launch screen; online launches when there's a
// usable session, otherwise routes to auth first. Returns nil when nothing is
// selected. The online decision uses the same noOnlineSession predicate that
// drives the launch label (footer + Overview button) so the label and the
// action can never disagree — including for a stale/unverified MSA session,
// which routes to auth rather than a doomed online launch. Shared by the
// Enter / "o" keys and the Overview Play button so every launch entry point
// behaves identically.
func (m *HomeModel) launchSelected(offline bool) tea.Cmd {
	inst := m.SelectedInstance()
	if inst == nil {
		return nil
	}
	if offline {
		return func() tea.Msg { return NavigateToLaunch{Instance: inst, Offline: true} }
	}
	if m.noOnlineSession() {
		return func() tea.Msg { return NavigateToAuth{} }
	}
	return func() tea.Msg { return NavigateToLaunch{Instance: inst, Offline: false} }
}

// buildFooter renders the block below the instance list: the auth/status line and
// the two-tier key-hint help. It does NOT include the transient banner (that sits
// between the list and this block and is accounted for separately). View and
// applyListSize both render through this helper so the measured footer height
// always matches what's drawn, preventing the list from overflowing the footer at
// narrow widths where the help wraps to extra lines.
func (m *HomeModel) buildFooter() string {
	// Auth line: avoid "signed in" when Microsoft session is invalid or unverified.
	authStatus := "Not signed in"
	authWarn := false
	if m.accounts != nil {
		if acc := m.accounts.GetActive(); acc != nil {
			switch acc.Type {
			case core.AccountTypeMSA:
				switch m.sessionRemote {
				case sessionRemoteInvalid:
					authStatus = fmt.Sprintf("%s — Microsoft sign-in required [a]", acc.Name)
					authWarn = true
				case sessionRemoteUncertain:
					authStatus = fmt.Sprintf("%s — can't verify session (network?) [a]", acc.Name)
					authWarn = true
				case sessionRemoteChecking:
					authStatus = fmt.Sprintf("Checking Microsoft session for %s…", acc.Name)
				default:
					authStatus = fmt.Sprintf("Signed in as %s", acc.Name)
				}
			case core.AccountTypeOffline:
				authStatus = fmt.Sprintf("Offline: %s", acc.Name)
			default:
				authStatus = fmt.Sprintf("Account: %s", acc.Name)
			}
		}
	}

	// Build help items based on auth status and pane focus. In detail focus the
	// arrows drive the panel rows, so advertise that vocabulary instead.
	var primaryItems, secondaryItems []KeyHint
	if m.paneFocus == focusDetail {
		primaryItems = []KeyHint{
			{"↑↓", "select"},
			{"↵", "open/add"},
			{"←→", "tabs"},
			{"tab", "back to list"},
		}
		secondaryItems = []KeyHint{
			{"esc", "list"},
			{"x", "actions"},
			{":", "jump"},
			{"q", "quit"},
		}
	} else {
		launchLabel := "launch"
		if m.noOnlineSession() {
			launchLabel = "login & play"
		}
		modsLabel := "mods"
		if inst := m.SelectedInstance(); inst != nil && !instanceSupportsModsBrowser(inst) {
			modsLabel = "mods (unsupported)"
		}
		primaryItems = []KeyHint{
			{"↵", launchLabel},
			{"n", "new"},
			{"m", modsLabel},
			{"→", "details"},
			{"tab", "focus panel"},
		}
		// `o play offline` is intentionally absent here: it's always visible in the
		// Overview pane's play controls (the list's companion view), so the bar
		// spends that slot on `s settings`, which otherwise has no visible
		// affordance. Keeping the item count steady preserves the narrow-width fit.
		secondaryItems = []KeyHint{
			{"p", "resource packs"},
			{"x", "actions"},
			{":", "jump"},
			{"a", "accounts"},
			{"s", "settings"},
			{"q", "quit"},
		}
	}

	help := lipgloss.JoinVertical(lipgloss.Left,
		KeyHints(m.width, primaryItems...),
		KeyHints(m.width, secondaryItems...),
	)

	statusColor := Active.Primary
	statusGlyph := GlyphDot
	if authWarn {
		statusColor = Active.Warning
		statusGlyph = GlyphWarn
	}
	status := lipgloss.NewStyle().
		Foreground(statusColor).
		Padding(1, 0).
		Render(statusGlyph + " " + authStatus)

	return lipgloss.JoinVertical(lipgloss.Left, status, help)
}

// homeFooterHeight measures the real footer (auth line + wrapped key hints) plus
// the transient-banner row when present. Measuring keeps the layout budget in sync
// with what View draws so narrow-width help wrapping can't overflow the screen.
func (m *HomeModel) homeFooterHeight() int {
	footerLines := lipgloss.Height(m.buildFooter())
	if m.transientBanner != "" {
		footerLines++
	}
	return footerLines
}

func (m *HomeModel) applyListSize() {
	footerH := m.homeFooterHeight()
	listH := homeListViewportHeight(m.height, m.width, footerH)

	if m.compactLayout {
		// Single full-width list panel; reserve one extra row for the condensed
		// summary line rendered between the panel and the footer. Use the full
		// terminal width (never wider — flooring up past m.width overflows narrow
		// panes). Inner width inside the panel border (+4 = border + one pad each side).
		m.list.SetSize(max(1, m.width-4), max(1, listH-1))
		return
	}

	// Two columns: the list bubble lives inside the list panel border.
	m.list.SetSize(m.listPaneW-4, listH)
}

// HasOverlay reports whether a transient modal (command palette, actions popup,
// or delete confirmation) is currently capturing input. The app uses this so the
// global quit key (q) doesn't fire — and discard palette text — while a modal is
// open; the modal's own esc/q handler should win instead.
func (m *HomeModel) HasOverlay() bool {
	return m.palette || m.actionsPopup || m.confirmDelete
}

// SelectedInstance returns the currently selected instance
func (m *HomeModel) SelectedInstance() *core.Instance {
	if item, ok := m.list.SelectedItem().(instanceItem); ok {
		return item.instance
	}
	return nil
}

// addRowSelected reports whether the trailing "+ New instance" action row is the
// current list selection (rather than a real instance). Used so Enter creates a
// new instance and the detail pane shows a tailored hint instead of trying to
// render a non-existent instance.
func (m *HomeModel) addRowSelected() bool {
	_, ok := m.list.SelectedItem().(addInstanceItem)
	return ok
}

// modCount returns the installed .jar count for inst, memoized per instance ID.
// Called only from View (never Update): the first view of a selection does the
// real (cheap) os.ReadDir via mods.ListInstalledJars; subsequent views hit the
// cache. The cache is reset in SetInstances.
func (m *HomeModel) modCount(inst *core.Instance) int {
	if inst == nil {
		return 0
	}
	if m.modCountCache == nil {
		m.modCountCache = map[string]int{}
	}
	if n, ok := m.modCountCache[inst.ID]; ok {
		return n
	}
	jars, _ := mods.ListInstalledJars(inst)
	n := len(jars)
	m.modCountCache[inst.ID] = n
	return n
}

// facetFor returns the cached facet bundle for inst, creating an empty one on
// first access. Called only from View.
func (m *HomeModel) facetFor(inst *core.Instance) *facetData {
	if m.facetCache == nil {
		m.facetCache = map[string]*facetData{}
	}
	fd, ok := m.facetCache[inst.ID]
	if !ok {
		fd = &facetData{}
		m.facetCache[inst.ID] = fd
	}
	return fd
}

// instanceWorlds returns the save folders for inst, reading from disk once per
// instance ID and caching the result. Called only from View (never Update).
func (m *HomeModel) instanceWorlds(inst *core.Instance) []worldEntry {
	if inst == nil {
		return nil
	}
	fd := m.facetFor(inst)
	if !fd.worldsLoaded {
		fd.worlds = readWorlds(inst.Path)
		fd.worldsLoaded = true
	}
	return fd.worlds
}

// instanceResourcePacks returns the installed resource packs for inst, reading
// from disk once per instance ID and caching the result. Called only from View
// (never Update).
func (m *HomeModel) instanceResourcePacks(inst *core.Instance) []resourcePackEntry {
	if inst == nil {
		return nil
	}
	fd := m.facetFor(inst)
	if !fd.packsLoaded {
		fd.packs = readResourcePacks(inst)
		fd.packsLoaded = true
	}
	return fd.packs
}

// instanceSupportsModsBrowser is true when the in-app Mods (Modrinth) screen applies.
// Expand this when additional loaders are wired up the same way.
func instanceSupportsModsBrowser(inst *core.Instance) bool {
	if inst == nil {
		return false
	}
	return loader.ParseKind(inst.Loader) == loader.KindFabric
}

// SetSize updates the dimensions of the home view and re-derives the two-column
// widths at the homeSplitMinWidth breakpoint.
func (m *HomeModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.compactLayout = width < homeSplitMinWidth
	if !m.compactLayout {
		m.listPaneW, m.detailPaneW = homeSplitWidths(width)
	} else {
		m.listPaneW, m.detailPaneW = 0, 0
	}
	m.applyListSize()
}

// Init implements tea.Model
func (m *HomeModel) Init() tea.Cmd {
	return nil
}

// Update implements tea.Model
func (m *HomeModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case InstancesLoaded:
		if msg.Error == nil {
			m.SetInstances(msg.Instances, msg.SelectID)
		}
		return m, nil

	case tea.KeyMsg:
		if m.transientBanner != "" {
			m.transientBanner = ""
			m.applyListSize()
		}

		// Overlay key handling — runs BEFORE the filter guard, the tab-cycle keys,
		// and the home key switch so home shortcuts can't leak into a modal. The
		// three overlays (palette, actions popup, delete-confirm) are mutually
		// exclusive: only one is ever armed at a time, and each is only opened from
		// the key switch below when none of the others is active and the list isn't
		// filtering.
		if m.palette {
			cmd, _ := m.updatePalette(msg)
			return m, cmd
		}
		if m.actionsPopup {
			cmd, _ := m.updateActionsPopup(msg)
			return m, cmd
		}

		// Handle delete confirmation mode
		if m.confirmDelete {
			if ConfirmKeyToggles(msg.String()) {
				m.deleteFocusYes = !m.deleteFocusYes
				return m, nil
			}
			switch msg.String() {
			case "y", "Y":
				inst := m.deleteTarget
				m.confirmDelete = false
				m.deleteTarget = nil
				return m, func() tea.Msg { return DeleteInstance{Instance: inst} }
			case "enter":
				if m.deleteFocusYes {
					inst := m.deleteTarget
					m.confirmDelete = false
					m.deleteTarget = nil
					return m, func() tea.Msg { return DeleteInstance{Instance: inst} }
				}
				m.confirmDelete = false
				m.deleteTarget = nil
				return m, nil
			case "n", "N", "esc", "q":
				m.confirmDelete = false
				m.deleteTarget = nil
				return m, nil
			}
			return m, nil
		}

		// Don't handle keys if filtering
		if m.list.FilterState() == list.Filtering {
			break
		}

		// Spatial pane/tab navigation. The dashboard is a left-to-right sequence of
		// columns — the instance list, then the detail tabs (Overview → Mods →
		// Resource packs). ←/→ (and h/l, [ /]) step between adjacent
		// columns while ↑/↓ move within the focused one, so both arrow axes act on
		// whichever pane has focus: → from the list enters the detail pane on
		// Overview and then walks rightward through the tabs (no wrap); ← walks back
		// and returns to the list from the leftmost tab. setActiveTab resets the
		// detail cursor on each tab switch. Disabled in compact mode (no detail pane).
		if !m.compactLayout {
			switch msg.String() {
			case "right", "l", "]":
				if m.paneFocus == focusList {
					m.focusDetailPane()
				} else if m.activeTab < homeTabCount-1 {
					m.setActiveTab(m.activeTab + 1)
				}
				return m, nil
			case "left", "h", "[":
				if m.paneFocus == focusDetail {
					if m.activeTab == tabOverview {
						m.focusListPane()
					} else {
						m.setActiveTab(m.activeTab - 1)
					}
				}
				return m, nil
			}
		}

		// Tab toggles which column owns ↑/↓ + Enter (the instance list vs the active
		// detail tab's rows); returning to the list restores the Overview summary.
		if msg.String() == "tab" {
			if m.paneFocus == focusList {
				m.focusDetailPane()
			} else {
				m.focusListPane()
			}
			return m, nil
		}

		// In detail focus, ↑/↓ drive the active tab's rows, Enter activates the
		// selected row, and Esc returns to the list — none may fall through to the
		// list bubble below. (Global letter hotkeys further down still run.)
		if m.paneFocus == focusDetail {
			switch msg.String() {
			case "esc":
				m.focusListPane()
				return m, nil
			case "up", "k":
				if n := m.detailRowCount(); n > 0 && m.detailCursor > 0 {
					m.detailCursor--
				}
				return m, nil
			case "down", "j":
				if n := m.detailRowCount(); n > 0 && m.detailCursor < n-1 {
					m.detailCursor++
				}
				return m, nil
			case "enter":
				// Overview has no rows — its Play button is the action, so Enter
				// launches (matching Enter from the list). Other tabs activate the row.
				if m.activeTab == tabOverview {
					return m, m.launchSelected(false)
				}
				return m, m.activateDetailRow()
			}
		}

		switch msg.String() {
		case ":":
			// Open the command palette (reached only when no overlay is active and
			// the list isn't filtering — both guarded above).
			m.openPalette()
			return m, textinput.Blink
		case "x":
			// Open the per-instance actions popup.
			if inst := m.SelectedInstance(); inst != nil {
				m.openActionsPopup(inst)
			}
			return m, nil
		case "e":
			// Bare `e` edits the selected instance directly.
			if inst := m.SelectedInstance(); inst != nil {
				return m, func() tea.Msg { return NavigateToEditInstance{Instance: inst} }
			}
			return m, nil
		}

		switch {
		case key.Matches(msg, m.keys.Launch):
			// Enter on the trailing "+ New instance" row opens the wizard; on a real
			// instance it launches.
			if m.addRowSelected() {
				return m, func() tea.Msg { return NavigateToNewInstance{} }
			}
			return m, m.launchSelected(false)
		case key.Matches(msg, m.keys.PlayOffline):
			return m, m.launchSelected(true)
		case key.Matches(msg, m.keys.NewInst):
			return m, func() tea.Msg { return NavigateToNewInstance{} }
		case key.Matches(msg, m.keys.Mods):
			if inst := m.SelectedInstance(); inst != nil {
				if !instanceSupportsModsBrowser(inst) {
					m.SetTransientBanner("Mods browser isn't available for this instance.")
					return m, nil
				}
				return m, func() tea.Msg { return NavigateToMods{Instance: inst} }
			}
		case key.Matches(msg, m.keys.ResPacks):
			// Resource packs work on ANY instance (vanilla or modded) — no gate.
			if inst := m.SelectedInstance(); inst != nil {
				return m, func() tea.Msg { return NavigateToResourcePacks{Instance: inst} }
			}
		case key.Matches(msg, m.keys.Settings):
			return m, func() tea.Msg { return NavigateToSettings{} }
		case key.Matches(msg, m.keys.Auth):
			return m, func() tea.Msg { return NavigateToAuth{} }
		case key.Matches(msg, m.keys.OpenFolder):
			if inst := m.SelectedInstance(); inst != nil {
				openInstanceFolder(inst.Path)
			}
		case key.Matches(msg, m.keys.Delete):
			if inst := m.SelectedInstance(); inst != nil {
				// Show confirmation prompt (Yes focused: Enter deletes; arrows or Tab change choice)
				m.confirmDelete = true
				m.deleteTarget = inst
				m.deleteFocusYes = true
				return m, nil
			}
		}
	}

	var cmd tea.Cmd
	m.list, cmd = m.list.Update(msg)
	// If list navigation moved to a different instance, drop any stale detail
	// cursor / focus so the right panel always reflects the new selection.
	if inst := m.SelectedInstance(); inst != nil && inst.ID != m.lastSelID {
		m.resetDetailFocus()
	}
	return m, cmd
}

// View implements tea.Model
func (m *HomeModel) View() string {
	if m.loading {
		return lipgloss.NewStyle().
			Foreground(Active.TextSubtle).
			Render("Loading instances…")
	}

	if len(m.instances) == 0 {
		// Delightful empty state with project intro
		title := lipgloss.NewStyle().
			Bold(true).
			Foreground(Active.Primary).
			Render(BrandGlyph + "  mctui")

		tagline := lipgloss.NewStyle().
			Foreground(Active.TextSubtle).
			Italic(true).
			Render("A terminal-based Minecraft launcher")

		// Size the rule to the content (cap it so it stays a tidy underline).
		ruleW := m.width / 3
		if ruleW > 40 {
			ruleW = 40
		}
		if ruleW < 12 {
			ruleW = 12
		}
		divider := Rule(ruleW)

		emptyMsg := lipgloss.NewStyle().
			Foreground(Active.Text).
			MarginTop(1).
			Render("No instances yet. Let's get started!")

		tips := lipgloss.NewStyle().
			MarginTop(1).
			Render(KeyHints(
				ruleW,
				KeyHint{"n", "new instance"},
				KeyHint{"a", "add account"},
				KeyHint{"s", "settings"},
				KeyHint{"q", "quit"},
			))

		content := lipgloss.JoinVertical(
			lipgloss.Center,
			title,
			tagline,
			"",
			divider,
			"",
			emptyMsg,
			tips,
		)

		// Center the content
		return lipgloss.Place(
			m.width,
			m.height,
			lipgloss.Center,
			lipgloss.Center,
			content,
		)
	}

	footer := m.buildFooter()

	// Title row: screen title left, instance count right (replaces the list's
	// internal title bar).
	titleW := m.width
	if !m.compactLayout {
		titleW = m.listPaneW + homeColumnGap + m.detailPaneW
	}
	screenTitle := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).
		Render(BrandGlyph + " Minecraft Instances")
	countLabel := lipgloss.NewStyle().Foreground(Active.TextMuted).
		Render(fmt.Sprintf("%d instance%s", len(m.instances), plural(len(m.instances))))
	titleRow := StatusBar(screenTitle, countLabel, titleW)

	columns := m.renderColumns()

	aboveStatus := []string{titleRow, "", columns}
	if m.transientBanner != "" {
		aboveStatus = append(aboveStatus, lipgloss.NewStyle().
			Foreground(Active.Warning).
			Render(GlyphWarn+" "+m.transientBanner))
	}
	aboveStatus = append(aboveStatus, footer)

	baseView := lipgloss.JoinVertical(lipgloss.Left, aboveStatus...)

	// Show the command palette overlay if active (mutually exclusive with the
	// other overlays).
	if m.palette {
		return boundToScreen(m.renderPalette(), m.width, m.height)
	}

	// Show the actions popup overlay if active.
	if m.actionsPopup && m.actionsTarget != nil {
		return boundToScreen(m.renderActionsPopup(), m.width, m.height)
	}

	// Show delete confirmation overlay if needed
	if m.confirmDelete && m.deleteTarget != nil {
		return ConfirmDialog{
			Title:    "Delete instance?",
			Message:  fmt.Sprintf("Delete %q?", m.deleteTarget.Name),
			Warning:  "This cannot be undone.",
			Confirm:  "Delete",
			Cancel:   "Cancel",
			Kind:     ConfirmDanger,
			FocusYes: m.deleteFocusYes,
		}.Render(m.width, m.height)
	}

	// Bound the rendered width to the terminal so a too-narrow pane can never
	// produce a line wider than m.width (which would wrap and corrupt the frame).
	return lipgloss.NewStyle().MaxWidth(m.width).Render(baseView)
}

// boundToScreen clamps a rendered string to the terminal envelope so modals
// (rendered via lipgloss.Place, which pads but does not truncate) can't overflow
// a very small terminal.
func boundToScreen(s string, w, h int) string {
	return lipgloss.NewStyle().MaxWidth(w).MaxHeight(h).Render(s)
}

// openInstanceFolder opens the instance folder in the system file manager
func openInstanceFolder(path string) {
	mcDir := filepath.Join(path, ".minecraft")
	_ = os.MkdirAll(mcDir, 0755)
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", mcDir)
	case "linux":
		cmd = exec.Command("xdg-open", mcDir)
	case "windows":
		cmd = exec.Command("explorer", mcDir)
	default:
		return
	}
	_ = cmd.Start()
}
