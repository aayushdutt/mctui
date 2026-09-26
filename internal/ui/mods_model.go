package ui

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/mods"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
)

// ModsModel: left = installed, right = Modrinth (split) or stacked when narrow.
type ModsModel struct {
	inst *core.Instance
	svc  *mods.Service

	width         int
	height        int
	compactLayout bool

	installed    list.Model
	installedErr string
	query        textinput.Model
	results      list.Model

	modsFocus modsPanel

	catalog             *mods.ModrinthCatalog
	cachedJars          []mods.InstalledJar
	installingProjectID string

	searchSeq    int
	searchCancel context.CancelFunc

	installCancel context.CancelFunc

	searching    bool
	searchErr    string
	searchNotice string

	lastTotalHits int

	installing bool
	installErr string
	installOK  string

	modsDialog         modsDialogKind
	modsDialogJar      string
	modsDialogFocusYes bool   // which confirm option is highlighted (opens on Cancel)
	libraryToast       string // short success line under the installed-mods pane (right pane stays Modrinth-only).

	libraryListW int
	resultsListW int

	palette CommandPalette

	// help is the `?` read-only key reference overlay (kit_help.go), dismissed by
	// ? / esc / q.
	help bool
}

// helpGroups returns the mods screen's bindings grouped for the `?` overlay,
// hand-built to match exactly what Update handles (pane cycling, list moves,
// install/remove/refresh/search) and the footer hints.
func (m *ModsModel) helpGroups() []HelpGroup {
	return []HelpGroup{
		{Title: "Navigate", Entries: []HelpEntry{
			{"↑↓ / j k", "move"},
			{"tab / ⇧tab", "switch pane"},
			{"← → / h l", "switch pane"},
		}},
		{Title: "Act", Entries: []HelpEntry{
			{"enter", "install mod"},
			{"d / backspace", "remove mod"},
			{"r", "refresh installed"},
			{"/", "search"},
		}},
		{Title: "General", Entries: []HelpEntry{
			{":", "command palette"},
			{"?", "help"},
			{"esc", "back home"},
		}},
	}
}

// NewModsModel builds a mod browser for inst. Callers must have cleared the
// mods gate ([CanBrowseMods]) first — every navigation into this screen does —
// so inst is always a Fabric instance and there is no "unsupported" fallback.
func NewModsModel(inst *core.Instance, client *api.ModrinthClient) *ModsModel {
	installedList := NewThemedList(ThemedListConfig{
		Accent: Active.Primary, AccentSoft: Active.Secondary, StatusBar: true,
	})
	installedList.Title = "Installed (0)"

	ti := textinput.New()
	ti.Placeholder = "Search Fabric mods, or leave empty for popular picks…"
	ti.CharLimit = 200
	ti.Width = 50
	ThemeTextInput(&ti)

	browseList := NewThemedList(ThemedListConfig{
		Accent: Active.Primary, AccentSoft: Active.Secondary, StatusBar: true,
	})
	browseList.Title = "Modrinth"

	m := &ModsModel{
		inst:      inst,
		svc:       mods.NewService(client),
		installed: installedList,
		query:     ti,
		results:   browseList,
		modsFocus: panelBrowse,
		catalog:   &mods.ModrinthCatalog{},
	}
	if inst != nil {
		_ = mods.EnsureModsDir(inst)
		m.reloadCatalogAndJars()
		m.refreshInstalled()
	}
	m.palette = NewCommandPalette("Mods · commands", "Run a command…", m.modsPaletteItems)
	return m
}

// modsPaletteItems supplies the mod screen's `:` command-palette verbs.
func (m *ModsModel) modsPaletteItems() []PaletteItem {
	return []PaletteItem{
		{Label: "Back to home", Run: func() tea.Cmd {
			return func() tea.Msg { return NavigateToHome{} }
		}},
		{Label: "Refresh installed", Run: func() tea.Cmd {
			m.refreshInstalled()
			return nil
		}},
		{Label: "Search mods", Run: func() tea.Cmd {
			m.libraryToast = ""
			m.modsFocus = panelQuery
			m.query.Focus()
			return textinput.Blink
		}},
	}
}

func (m *ModsModel) reloadCatalogAndJars() {
	cat, err := mods.LoadModrinthCatalog(m.inst)
	if err != nil || cat == nil {
		m.catalog = &mods.ModrinthCatalog{}
	} else {
		m.catalog = cat
	}
	jars, _ := mods.ListInstalledJars(m.inst)
	m.cachedJars = jars
}

func (m *ModsModel) rowNoteForHit(hit api.SearchHit) string {
	if m.installingProjectID == hit.ProjectID {
		return modRowInstalling
	}
	if mods.IsModrinthProjectInstalled(m.catalog, m.cachedJars, hit.ProjectID, hit.Slug) {
		return modRowInstalled
	}
	return ""
}

func (m *ModsModel) hitsToItems(hits []api.SearchHit) []list.Item {
	items := make([]list.Item, 0, len(hits))
	for _, h := range hits {
		h := h
		note := m.rowNoteForHit(h)
		items = append(items, modListItem{hit: h, rowNote: note})
	}
	return items
}

func (m *ModsModel) rebuildBrowseBadges() {
	raw := m.results.Items()
	next := make([]list.Item, len(raw))
	for i, it := range raw {
		if li, ok := it.(modListItem); ok {
			li.rowNote = m.rowNoteForHit(li.hit)
			next[i] = li
		} else {
			next[i] = it
		}
	}
	m.results.SetItems(next)
}

func (m *ModsModel) clearModsDialog() {
	m.modsDialog = modsDialogNone
	m.modsDialogJar = ""
}

func (m *ModsModel) syncModsDialogSelection() {
	if m.modsDialog != modsDialogConfirmRemoveJar {
		return
	}
	it, ok := m.installed.SelectedItem().(modInstalledItem)
	if !ok || it.jar.Name != m.modsDialogJar {
		m.clearModsDialog()
	}
}

func (m *ModsModel) openRemoveConfirmDialog() bool {
	if len(m.installed.Items()) == 0 {
		return false
	}
	it, ok := m.installed.SelectedItem().(modInstalledItem)
	if !ok {
		return false
	}
	m.libraryToast = ""
	m.modsDialog = modsDialogConfirmRemoveJar
	m.modsDialogJar = it.jar.Name
	m.modsDialogFocusYes = false
	return true
}

func (m *ModsModel) removeConfirmedJar(name string) {
	idx := m.installed.Index()
	if err := mods.RemoveInstalledJar(m.inst, name); err != nil {
		m.installedErr = "Couldn't remove jar: " + err.Error()
		m.libraryToast = ""
		return
	}
	m.installedErr = ""
	if err := mods.DropCatalogEntriesForJar(m.inst, name); err != nil {
		m.installedErr = fmt.Sprintf("Removed jar; catalog cleanup: %v", err)
	}
	m.installOK = ""
	m.installErr = ""
	m.libraryToast = fmt.Sprintf(`Removed "%s" from this instance.`, name)
	m.loadInstalledJarList()
	if n := len(m.installed.Items()); n > 0 {
		if idx >= n {
			idx = n - 1
		}
		m.installed.Select(idx)
	}
	m.rebuildBrowseBadges()
}

func (m *ModsModel) loadInstalledJarList() {
	m.reloadCatalogAndJars()
	m.installedErr = ""
	jars, err := mods.ListInstalledJars(m.inst)
	if err != nil {
		m.installedErr = "Couldn't read installed mods: " + err.Error()
		m.installed.SetItems(nil)
		m.installed.Title = "Installed"
		m.rebuildBrowseBadges()
		return
	}
	m.cachedJars = jars
	items := make([]list.Item, 0, len(jars))
	for _, j := range jars {
		j := j
		items = append(items, modInstalledItem{jar: j})
	}
	m.installed.SetItems(items)
	m.installed.Title = fmt.Sprintf("Installed (%d)", len(jars))
	m.rebuildBrowseBadges()
}

func (m *ModsModel) refreshInstalled() {
	m.clearModsDialog()
	m.libraryToast = ""
	m.loadInstalledJarList()
}

// SetSize updates layout dimensions.
func (m *ModsModel) SetSize(w, h int) {
	if w < 1 {
		w = 1
	}
	if h < 1 {
		h = 1
	}
	m.width, m.height = w, h
	m.compactLayout = w < splitMinWidth
	if m.compactLayout {
		m.query.Width = min(60, max(24, w-10))
		listW := max(20, w-6)
		m.libraryListW = listW
		m.resultsListW = listW
		libH, browseH := modsCompactListHeights(h, w)
		m.installed.SetSize(listW, libH)
		m.results.SetSize(listW, browseH)
		return
	}
	leftW := max(28, min(40, w*32/100))
	rightInner := w - leftW - 5
	if rightInner < 34 {
		leftW = max(24, w*30/100)
		rightInner = w - leftW - 5
	}
	m.query.Width = min(58, max(24, rightInner-4))
	// Zones have no border to inset for, so the lists fill their panes fully
	// (reclaiming the ~4 columns the old rounded borders took).
	lw := max(22, leftW)
	rw := max(28, rightInner)
	m.libraryListW = lw
	m.resultsListW = rw
	listH := modsSplitListViewportHeight(h, w)
	m.installed.SetSize(lw, listH)
	m.results.SetSize(rw, listH)
}

func (m *ModsModel) cancelSearch() {
	if m.searchCancel != nil {
		m.searchCancel()
		m.searchCancel = nil
	}
}

func (m *ModsModel) cancelInstallDownload() {
	if m.installCancel != nil {
		m.installCancel()
		m.installCancel = nil
	}
}

// CancelPending stops in-flight Modrinth search and mod download. Call before discarding the model (e.g. leaving the screen).
func (m *ModsModel) CancelPending() {
	m.cancelSearch()
	m.cancelInstallDownload()
}

// Init implements tea.Model.
func (m *ModsModel) Init() tea.Cmd {
	m.refreshInstalled()
	m.cancelSearch()
	m.cancelInstallDownload()
	m.searchSeq++
	seq := m.searchSeq
	m.searching = true
	m.searchErr = ""
	m.searchNotice = ""
	m.results.Title = "Modrinth — popular"
	ctx, cancel := context.WithCancel(context.Background())
	m.searchCancel = cancel
	browse := func() tea.Msg {
		res, err := m.svc.SearchFabricMods(ctx, m.inst, "", 0)
		if err != nil && errors.Is(err, context.Canceled) {
			return modSearchStaleMsg{}
		}
		return modSearchResultMsg{seq: seq, result: res, err: err}
	}
	return browse
}

func (m *ModsModel) footerHelpItems() []KeyHint {
	if m.compactLayout {
		return modsCompactFooterItems(m.height, m.width)
	}
	return modsHelpItemsPick(m.height, m.width)
}

func (m *ModsModel) cycleTab() {
	m.clearModsDialog()
	switch m.modsFocus {
	case panelInstalled:
		m.modsFocus = panelQuery
		m.query.Focus()
	case panelQuery:
		m.query.Blur()
		m.modsFocus = panelBrowse
	case panelBrowse:
		m.modsFocus = panelInstalled
	}
}

func (m *ModsModel) rightColumnFocused() bool {
	return m.modsFocus == panelBrowse || m.modsFocus == panelQuery
}

func (m *ModsModel) runSearchQuery() tea.Cmd {
	q := strings.TrimSpace(m.query.Value())
	m.searching = true
	m.searchErr = ""
	m.searchNotice = ""
	seq := m.searchSeq
	ctx, cancel := context.WithCancel(context.Background())
	m.searchCancel = cancel
	return func() tea.Msg {
		res, err := m.svc.SearchFabricMods(ctx, m.inst, q, 0)
		if err != nil && errors.Is(err, context.Canceled) {
			return modSearchStaleMsg{}
		}
		return modSearchResultMsg{seq: seq, result: res, err: err}
	}
}

func (m *ModsModel) installModCmd(projectID, title string) tea.Cmd {
	m.cancelInstallDownload()
	m.searchNotice = ""
	m.installing = true
	m.installErr = ""
	m.installOK = ""
	inst := m.inst
	svc := m.svc
	ctx, cancel := context.WithCancel(context.Background())
	m.installCancel = cancel
	ch := make(chan ModInstallDoneMsg, 1)
	go func() {
		defer cancel()
		report, err := svc.InstallFabricModWithDeps(ctx, inst, projectID)
		ch <- ModInstallDoneMsg{
			ProjectID: projectID,
			Title:     title,
			Report:    report,
			Err:       err,
		}
	}()
	return func() tea.Msg {
		return <-ch
	}
}
