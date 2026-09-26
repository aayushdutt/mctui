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
	"strings"
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

	// lastSelID tracks the selected instance ID across Updates so the detail
	// focus can be reset when the list cursor moves to a new instance.
	lastSelID string

	// facetCache memoizes the disk-backed facet readers (worlds, resource packs) per
	// instance ID, populated lazily on first view of a (instance, tab) and
	// invalidated wholesale in SetInstances. NEVER read in Update's key path.
	facetCache map[string]*facetData

	// Microsoft session hint from background check (MSA accounts only)
	sessionRemote sessionRemoteLine

	// playingID is the instance whose game is running after the user left the
	// launch screen ("" when none). Enter on it reopens the launch screen.
	playingID string

	// gatingName is the instance whose online launch is waiting on the session
	// check; while set, the status line says so (Enter is ignored meanwhile).
	gatingName string

	// notice is a persistent message (startup recovery, a crash while away)
	// shown until the next key press — unlike transientBanner, which expires
	// on a timer and could go unseen.
	notice []string

	// transientBanner is a one-line notice (e.g. session gate network error).
	// It auto-expires after homeBannerTTL via a tea.Tick keyed on bannerGen, so
	// it survives unrelated keypresses instead of vanishing on the next key.
	transientBanner string
	bannerGen       int

	// Delete confirmation state
	confirmDelete  bool
	deleteTarget   *core.Instance
	deleteFocusYes bool // which option arrows / Enter apply to (opens on Cancel; see armDelete)

	// Actions popup state (home_actions.go). Mutually exclusive with the other
	// overlays — never armed while confirmDelete or palette is active.
	actionsPopup  bool
	actionsTarget *core.Instance
	actionsCursor int

	// Command-palette state (home_palette.go + kit_palette.go). Mutually exclusive
	// with the other overlays; the textinput owns keystrokes while it is Active.
	palette CommandPalette

	// help is the `?` full-screen key reference overlay (kit_help.go). Read-only,
	// dismissed by ? / esc / q.
	help bool
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

// homeKeyMap is the single source of truth for the home screen's bindings. Every
// key the screen acts on lives here (no raw-string switches in Update), so the
// `?` help overlay — which renders straight from these bindings — can never
// advertise a key the screen doesn't handle, or omit one it does.
type homeKeyMap struct {
	Move        key.Binding // ↑↓/jk within the focused pane (handled by the list bubble)
	Page        key.Binding // pgup/pgdn pages the instance list (handled by the list bubble)
	FocusPane   key.Binding // tab: toggle list ↔ detail focus
	StepRight   key.Binding // →/l/]: focus detail, then walk tabs right
	StepLeft    key.Binding // ←/h/[: walk tabs left, then back to the list
	Launch      key.Binding
	PlayOffline key.Binding
	NewInst     key.Binding
	Mods        key.Binding
	ResPacks    key.Binding
	Settings    key.Binding
	Edit        key.Binding
	Delete      key.Binding
	OpenFolder  key.Binding
	Actions     key.Binding
	Palette     key.Binding
	Auth        key.Binding
	Back        key.Binding
	Help        key.Binding
}

func defaultHomeKeyMap() homeKeyMap {
	return homeKeyMap{
		Move: key.NewBinding(
			key.WithKeys("up", "down", "j", "k"),
			key.WithHelp("↑↓ / j k", "move"),
		),
		Page: key.NewBinding(
			key.WithKeys("pgup", "pgdown"),
			key.WithHelp("pgup / pgdn", "page"),
		),
		FocusPane: key.NewBinding(
			key.WithKeys("tab"),
			key.WithHelp("tab", "focus list ↔ panel"),
		),
		StepRight: key.NewBinding(
			key.WithKeys("right", "l", "]"),
			key.WithHelp("→ / l / ]", "details · next tab"),
		),
		StepLeft: key.NewBinding(
			key.WithKeys("left", "h", "["),
			key.WithHelp("← / h / [", "back · prev tab"),
		),
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
			key.WithHelp("n", "new instance"),
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
		Edit: key.NewBinding(
			key.WithKeys("e"),
			key.WithHelp("e", "edit instance"),
		),
		Delete: key.NewBinding(
			key.WithKeys("d"),
			key.WithHelp("d", "delete instance"),
		),
		OpenFolder: key.NewBinding(
			key.WithKeys("f"),
			key.WithHelp("f", "open folder"),
		),
		Actions: key.NewBinding(
			key.WithKeys("x"),
			key.WithHelp("x", "actions"),
		),
		Palette: key.NewBinding(
			key.WithKeys(":"),
			key.WithHelp(":", "command palette"),
		),
		Auth: key.NewBinding(
			key.WithKeys("a"),
			key.WithHelp("a", "accounts"),
		),
		Back: key.NewBinding(
			key.WithKeys("esc"),
			key.WithHelp("esc", "clear filter · back"),
		),
		Help: key.NewBinding(
			key.WithKeys("?"),
			key.WithHelp("?", "help"),
		),
	}
}

// helpGroups returns the home bindings grouped for the `?` overlay, built
// straight from homeKeyMap so the overlay always matches what Update handles.
func (m *HomeModel) helpGroups() []HelpGroup {
	return []HelpGroup{
		helpGroup("Navigate", m.keys.Move, m.keys.Page, m.keys.FocusPane, m.keys.StepRight, m.keys.StepLeft),
		helpGroup("Act", m.keys.Launch, m.keys.PlayOffline, m.keys.NewInst, m.keys.Edit, m.keys.Delete, m.keys.OpenFolder),
		helpGroup("Screens", m.keys.Mods, m.keys.ResPacks, m.keys.Settings, m.keys.Auth),
		helpGroup("General", m.keys.Actions, m.keys.Palette, m.keys.Back, m.keys.Help,
			key.NewBinding(key.WithKeys("/"), key.WithHelp("/", "filter")),
			key.NewBinding(key.WithKeys("q"), key.WithHelp("q", "quit")),
		),
	}
}

// instanceItem represents a Minecraft instance in the list
type instanceItem struct {
	instance *core.Instance
	playing  bool // its game is running (launched, then left via esc)
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
	if i.playing {
		// Lead with it so narrow columns truncate the details, not the state.
		return fmt.Sprintf("%s Playing • %s • %s", GlyphDot, i.instance.Version, loader)
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
	// The list's default paging keys (h/l/←/→/f/d/b/u) collide with home's own
	// letters and pane/tab steps; page with PgUp/PgDn only.
	l.KeyMap.PrevPage.SetKeys("pgup")
	l.KeyMap.NextPage.SetKeys("pgdown")
	ThemeListChrome(&l)

	m := &HomeModel{
		list:          l,
		keys:          defaultHomeKeyMap(),
		loading:       true,
		modCountCache: map[string]int{},
		facetCache:    map[string]*facetData{},
	}
	m.palette = NewCommandPalette("Command palette", "Jump to instance or run a command…", m.homePaletteItems)
	return m
}

// ApplyTheme re-dresses the (long-lived) home list from the active theme. The
// app calls this after the theme changes so the home screen — which, unlike the
// per-entry screens, is not rebuilt on navigation — picks up the new palette.
func (m *HomeModel) ApplyTheme() {
	base := ThemeListDelegate(Active.Primary, Active.Secondary)
	m.list.SetDelegate(&homeInstanceDelegate{DefaultDelegate: base})
	ThemeListChrome(&m.list)
}

// SetInstances updates the instance list. If selectID is non-empty, the cursor
// moves to that instance (usually index 0 for a new instance); otherwise it
// stays on whatever is selected now — the same instance wherever it sorts to,
// or the "+ New instance" row. The returned command re-applies an active filter
// (the list clears its filtered view on SetItems); callers must run it.
func (m *HomeModel) SetInstances(instances []*core.Instance, selectID string) tea.Cmd {
	// Capture the live selection before the items change: the list re-sorts by
	// recency, so keeping the row index would land on another instance (e.g.
	// after playing one) and Enter would launch the wrong one.
	prevID := ""
	if inst := m.SelectedInstance(); inst != nil {
		prevID = inst.ID
	}
	// Only a deliberate choice: with no instances, + New is the only row.
	onAddRow := m.addRowSelected() && len(m.instances) > 0

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
		items = append(items, instanceItem{instance: inst, playing: inst.ID == m.playingID})
	}
	// Trailing "+ New instance" action row, always last.
	items = append(items, addInstanceItem{})
	cmd := m.list.SetItems(items)

	if m.list.FilterState() != list.Unfiltered {
		// Cursor positions index the filtered view, which SetItems cleared and
		// cmd rebuilds; leave the cursor alone rather than land on a wrong row.
		return cmd
	}

	target := selectID
	if target == "" {
		target = prevID
	}
	found := false
	for i, inst := range instances {
		if target != "" && inst.ID == target {
			m.list.Select(i)
			found = true
			break
		}
	}
	switch {
	case found:
	case selectID != "":
		m.list.Select(0)
	case onAddRow:
		m.list.Select(len(instances))
	}
	// Only a different selection resets the detail pane (a background reload,
	// e.g. a game exiting, shouldn't yank focus back to the list).
	if cur := m.SelectedInstance(); cur == nil || cur.ID != prevID {
		m.resetDetailFocus()
	}
	return cmd
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

// homeBannerTTL is how long a transient banner stays before auto-expiring.
const homeBannerTTL = 4 * time.Second

// homeBannerExpireMsg clears the banner iff it is still the one that scheduled
// this tick (gen match) — a newer banner supersedes an older pending expiry.
type homeBannerExpireMsg struct{ gen int }

// SetNotice shows lines as a persistent notice above the footer (and on the
// empty-state screen) until the next key press. No lines clears it.
func (m *HomeModel) SetNotice(lines ...string) {
	m.notice = lines
	m.applyListSize()
}

// noticeView renders the notice wrapped to the screen width ("" when none).
func (m *HomeModel) noticeView() string {
	if len(m.notice) == 0 {
		return ""
	}
	style := lipgloss.NewStyle().Foreground(Active.Warning).Width(max(1, m.width))
	rows := make([]string, 0, len(m.notice)+1)
	for _, line := range m.notice {
		rows = append(rows, style.Render(GlyphWarn+" "+line))
	}
	rows = append(rows, lipgloss.NewStyle().Foreground(Active.TextMuted).Render("  (press any key to dismiss)"))
	return lipgloss.JoinVertical(lipgloss.Left, rows...)
}

// SetTransientBanner shows a short notice above the footer and returns a command
// that clears it after homeBannerTTL. The banner no longer vanishes on the next
// keypress; callers should run the returned cmd (batch it with theirs). An empty
// string clears immediately and returns nil.
func (m *HomeModel) SetTransientBanner(s string) tea.Cmd {
	m.transientBanner = s
	m.bannerGen++
	m.applyListSize()
	if s == "" {
		return nil
	}
	gen := m.bannerGen
	return tea.Tick(homeBannerTTL, func(time.Time) tea.Msg {
		return homeBannerExpireMsg{gen: gen}
	})
}

// SetPlaying marks the instance whose game is running ("" clears it). The
// returned command re-applies an active filter (see SetInstances).
func (m *HomeModel) SetPlaying(id string) tea.Cmd {
	if m.playingID == id {
		return nil
	}
	m.playingID = id
	items := m.list.Items()
	for i, it := range items {
		if ii, ok := it.(instanceItem); ok {
			ii.playing = ii.instance.ID == id
			items[i] = ii
		}
	}
	return m.list.SetItems(items)
}

// SetLaunchGate shows (name != "") or clears the "verifying session" status
// for an online launch that is waiting on the session check.
func (m *HomeModel) SetLaunchGate(name string) {
	m.gatingName = name
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
	if m.playingID != "" {
		// A game is running: the app reopens its screen (this instance) or says
		// it's still running (another one). No session check or sign-in detour.
		return func() tea.Msg { return NavigateToLaunch{Instance: inst, Offline: true} }
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

	if m.gatingName != "" {
		authStatus = fmt.Sprintf("Verifying Microsoft session to play %s…", m.gatingName)
		authWarn = false
	}

	// One footer line: the top six context-relevant hints, always ending in
	// `? help`. Everything else lives in the `?` overlay and the `:` palette, so
	// the bar stays a single line instead of an eleven-binding wall. In detail
	// focus the arrows drive the panel rows, so advertise that vocabulary instead.
	var items []KeyHint
	if m.paneFocus == focusDetail {
		items = []KeyHint{
			{"↑↓", "select"},
			{"↵", "open"},
			{"←→", "tabs"},
			{"tab", "list"},
			{":", "palette"},
			{"?", "help"},
		}
	} else {
		launchLabel := "play"
		if inst := m.SelectedInstance(); inst != nil && inst.ID == m.playingID {
			launchLabel = "view game"
		} else if m.noOnlineSession() {
			launchLabel = "login & play"
		}
		items = []KeyHint{
			{"↵", launchLabel},
			{"n", "new"},
			{"x", "actions"},
			{":", "palette"},
			{"/", "filter"},
			{"?", "help"},
		}
	}

	help := KeyHints(m.width, items...)

	statusColor := Active.Primary
	statusGlyph := GlyphDot
	if authWarn {
		statusColor = Active.Warning
		statusGlyph = GlyphWarn
	}
	// One blank line separates the list/columns from the status; the hints sit
	// directly under it. Keeps the whole footer to three rows (gap · status · hints).
	status := lipgloss.NewStyle().
		Foreground(statusColor).
		MarginTop(1).
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
	if n := m.noticeView(); n != "" {
		footerLines += lipgloss.Height(n)
	}
	return footerLines
}

func (m *HomeModel) applyListSize() {
	footerH := m.homeFooterHeight()
	listH := homeListViewportHeight(m.height, m.width, footerH)

	if m.compactLayout {
		// Single full-width list Zone; reserve one extra row for the condensed
		// summary line rendered between the pane and the footer. The Zone has no
		// border, so the list fills the full content width.
		m.list.SetSize(max(1, m.width), max(1, listH-1))
		return
	}

	// Two columns: the list bubble fills its Zone (no border to inset for).
	m.list.SetSize(m.listPaneW, listH)
}

// armDelete opens the delete-instance confirmation for inst with Cancel
// focused: deleting removes the whole folder, worlds included, so d then Enter
// must not do it. Every entry point (d, the x popup, the palette) goes here.
func (m *HomeModel) armDelete(inst *core.Instance) {
	m.confirmDelete = true
	m.deleteTarget = inst
	m.deleteFocusYes = false
}

// deleteSummary says what deleting inst destroys, e.g.
// "Removes its folder: 2 worlds · 5 mods · 4h 12m played."
func (m *HomeModel) deleteSummary(inst *core.Instance) string {
	var parts []string
	if n := len(m.instanceWorlds(inst)); n > 0 {
		parts = append(parts, fmt.Sprintf("%d world%s", n, plural(n)))
	}
	if n := m.modCount(inst); n > 0 {
		parts = append(parts, fmt.Sprintf("%d mod%s", n, plural(n)))
	}
	if inst.PlayTime > 0 {
		parts = append(parts, formatPlayTime(inst.PlayTime)+" played")
	}
	if len(parts) == 0 {
		return "Removes its folder."
	}
	return "Removes its folder: " + strings.Join(parts, " · ") + "."
}

// CapturesInput reports whether home is consuming keystrokes as input — an
// overlay, or typing into the list filter — so global keys (q = quit) must not
// fire.
func (m *HomeModel) CapturesInput() bool {
	return m.HasOverlay() || m.list.FilterState() == list.Filtering
}

// HasOverlay reports whether a transient modal (command palette, actions popup,
// delete confirmation, or help) is currently capturing input. See CapturesInput.
func (m *HomeModel) HasOverlay() bool {
	return m.palette.Active || m.actionsPopup || m.confirmDelete || m.help
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

// CanBrowseMods is the single mods-browser gate: it reports whether the in-app
// Modrinth screen applies to inst and, when it doesn't, a one-line reason for the
// banner. Every entry point into the mods screen (home `m` key, the detail
// preview's Enter, the command palette) routes through this one helper, so the
// rule lives in exactly one place. Expand it when more loaders are wired up.
func CanBrowseMods(inst *core.Instance) (bool, string) {
	if inst == nil {
		return false, "No instance selected."
	}
	if loader.ParseKind(inst.Loader) != loader.KindFabric {
		return false, "Mods browser isn't available for this instance."
	}
	return true, ""
}

// instanceSupportsModsBrowser is the bool-only view of [CanBrowseMods], for
// display choices (e.g. the detail preview's empty-state hint) that don't need
// the reason string.
func instanceSupportsModsBrowser(inst *core.Instance) bool {
	ok, _ := CanBrowseMods(inst)
	return ok
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
		if msg.Error != nil {
			// Don't fail silently: the list would otherwise just stay on its last
			// state (or "Loading…") with no explanation.
			m.loading = false
			return m, m.SetTransientBanner("Couldn't load instances: " + msg.Error.Error())
		}
		return m, m.SetInstances(msg.Instances, msg.SelectID)

	case homeBannerExpireMsg:
		// Clear only if no newer banner has superseded this tick.
		if msg.gen == m.bannerGen {
			m.transientBanner = ""
			m.applyListSize()
		}
		return m, nil

	case tea.KeyMsg:
		if len(m.notice) > 0 {
			// Any key dismisses the notice (and still does its normal job).
			m.SetNotice()
		}

		// Overlay key handling — runs BEFORE the filter guard, the tab-cycle keys,
		// and the home key switch so home shortcuts can't leak into a modal. The
		// three overlays (palette, actions popup, delete-confirm) are mutually
		// exclusive: only one is ever armed at a time, and each is only opened from
		// the key switch below when none of the others is active and the list isn't
		// filtering.
		if m.palette.Active {
			return m, m.palette.Update(msg)
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

		// Help overlay: read-only, so it swallows every key and only ? / esc / q
		// dismiss it. Checked alongside the other overlays, before the filter guard.
		if m.help {
			switch {
			case key.Matches(msg, m.keys.Help), key.Matches(msg, m.keys.Back), msg.String() == "q":
				m.help = false
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
			switch {
			case key.Matches(msg, m.keys.StepRight):
				if m.paneFocus == focusList {
					m.focusDetailPane()
				} else if m.activeTab < homeTabCount-1 {
					m.setActiveTab(m.activeTab + 1)
				}
				return m, nil
			case key.Matches(msg, m.keys.StepLeft):
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
		if key.Matches(msg, m.keys.FocusPane) {
			if m.paneFocus == focusList {
				m.focusDetailPane()
			} else {
				m.focusListPane()
			}
			return m, nil
		}

		// In detail focus the tabs are read-only previews (not row-navigable):
		// Esc returns to the list, ↑/↓ are inert (walk tabs with ←/→), and Enter
		// activates the tab — Overview launches, the Mods / Resource packs previews
		// open their manager. None may fall through to the list bubble below.
		// (Global letter hotkeys further down still run.)
		if m.paneFocus == focusDetail {
			switch msg.String() {
			case "esc":
				m.focusListPane()
				return m, nil
			case "up", "k", "down", "j":
				return m, nil
			case "enter":
				return m, m.activateDetailTab()
			}
		}

		switch {
		case key.Matches(msg, m.keys.Help):
			// Open the read-only key reference overlay.
			m.help = true
			return m, nil
		case key.Matches(msg, m.keys.Back):
			// Esc = one step back. In list focus (detail-focus esc is handled above):
			// clear an applied filter first, otherwise the list is already the root —
			// hint how to actually leave rather than dying silently.
			if m.list.FilterState() != list.Unfiltered {
				m.list.ResetFilter()
				return m, nil
			}
			return m, m.SetTransientBanner("Press q to quit.")
		case key.Matches(msg, m.keys.Palette):
			// Open the command palette (reached only when no overlay is active and
			// the list isn't filtering — both guarded above).
			m.openPalette()
			return m, textinput.Blink
		case key.Matches(msg, m.keys.Actions):
			// Open the per-instance actions popup.
			if inst := m.SelectedInstance(); inst != nil {
				m.openActionsPopup(inst)
			}
			return m, nil
		case key.Matches(msg, m.keys.Edit):
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
				if ok, reason := CanBrowseMods(inst); !ok {
					return m, m.SetTransientBanner(reason)
				}
				return m, func() tea.Msg { return NavigateToMods{Instance: inst} }
			}
			return m, nil
		case key.Matches(msg, m.keys.ResPacks):
			// Resource packs work on ANY instance (vanilla or modded) — no gate.
			if inst := m.SelectedInstance(); inst != nil {
				return m, func() tea.Msg { return NavigateToResourcePacks{Instance: inst} }
			}
			return m, nil
		case key.Matches(msg, m.keys.Settings):
			return m, func() tea.Msg { return NavigateToSettings{} }
		case key.Matches(msg, m.keys.Auth):
			return m, func() tea.Msg { return NavigateToAuth{} }
		case key.Matches(msg, m.keys.OpenFolder):
			if inst := m.SelectedInstance(); inst != nil {
				openInstanceFolder(inst.Path)
			}
			return m, nil
		case key.Matches(msg, m.keys.Delete):
			if inst := m.SelectedInstance(); inst != nil {
				m.armDelete(inst)
			}
			return m, nil
		}
		// Handled home hotkeys return above; only unclaimed keys (↑/↓, paging,
		// filter) reach the list.
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

		// Minecraft-title-screen wink: the process's splash line under the wordmark.
		splash := SplashLine()

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
			splash,
			"",
			divider,
			"",
			emptyMsg,
			tips,
		)
		// Notices must show here too: with no instances (e.g. a config pointing
		// at another data folder was just reset) this is the only screen.
		notice := m.noticeView()
		placeH := m.height
		if notice != "" {
			placeH = max(1, m.height-lipgloss.Height(notice))
		}

		// Center the content
		centered := lipgloss.Place(
			m.width,
			placeH,
			lipgloss.Center,
			lipgloss.Center,
			content,
		)
		if notice != "" {
			return lipgloss.JoinVertical(lipgloss.Left, notice, centered)
		}
		return centered
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
	if n := m.noticeView(); n != "" {
		aboveStatus = append(aboveStatus, n)
	}
	if m.transientBanner != "" {
		aboveStatus = append(aboveStatus, lipgloss.NewStyle().
			Foreground(Active.Warning).
			Render(GlyphWarn+" "+m.transientBanner))
	}
	aboveStatus = append(aboveStatus, footer)

	baseView := lipgloss.JoinVertical(lipgloss.Left, aboveStatus...)

	// Overlays composite over a dimmed copy of the instance list so context (which
	// instance you're deleting / jumping to) stays visible behind the modal.
	if m.help {
		return Composite(baseView, HelpOverlay("Home", m.helpGroups(), m.width, m.height), m.width, m.height)
	}

	// Command palette (mutually exclusive with the other overlays).
	if m.palette.Active {
		return Composite(baseView, m.palette.View(m.width, m.height), m.width, m.height)
	}

	// Per-instance actions popup.
	if m.actionsPopup && m.actionsTarget != nil {
		return Composite(baseView, m.renderActionsPopup(), m.width, m.height)
	}

	// Delete confirmation.
	if m.confirmDelete && m.deleteTarget != nil {
		dialog := ConfirmDialog{
			Title:    "Delete instance?",
			Message:  fmt.Sprintf("Delete %q?\n%s", m.deleteTarget.Name, m.deleteSummary(m.deleteTarget)),
			Warning:  "This cannot be undone.",
			Confirm:  "Delete",
			Cancel:   "Cancel",
			Kind:     ConfirmDanger,
			FocusYes: m.deleteFocusYes,
		}.Render(m.width, m.height)
		return Composite(baseView, dialog, m.width, m.height)
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

// openInstanceFolder opens the instance's .minecraft folder in the OS file
// manager. A var so tests can stub it (pressing f must not open a real window).
var openInstanceFolder = func(path string) {
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
