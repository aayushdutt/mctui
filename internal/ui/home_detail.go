package ui

import (
	"fmt"
	"strings"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/mods"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// loaderDisplay returns a human label for an instance's loader, capitalized, with
// the loader version appended when set (e.g. "Fabric 0.16.10").
func loaderDisplay(inst *core.Instance) string {
	name := inst.Loader
	switch name {
	case "", "vanilla":
		return "Vanilla"
	case "fabric":
		name = "Fabric"
	case "forge":
		name = "Forge"
	case "quilt":
		name = "Quilt"
	case "neoforge":
		name = "NeoForge"
	}
	if v := inst.LoaderVer; v != "" {
		return name + " " + v
	}
	return name
}

// kvRow renders one detail row: a fixed-width muted label column followed by a
// foreground value. labelW right-pads the label so values align in a column.
func kvRow(label, value string, labelW int) string {
	lbl := lipgloss.NewStyle().Foreground(Active.TextMuted).Width(labelW).Render(label)
	val := lipgloss.NewStyle().Foreground(Active.Text).Render(value)
	return lipgloss.JoinHorizontal(lipgloss.Top, lbl, val)
}

// statusLabel maps the cached/download state to a glyph + label for the Overview
// body. It is deliberately understated — neutral text, no semantic fill — since
// the colored at-a-glance signal already lives in the top statusBadge; the
// Overview card stays calm.
func statusLabel(inst *core.Instance) string {
	txt := lipgloss.NewStyle().Foreground(Active.TextSubtle)
	if inst.IsFullyDownloaded {
		return txt.Render(GlyphDone + " Ready (cached)")
	}
	return txt.Render(GlyphPending + " Needs download")
}

// statusBadge is the compact ▸ Ready / ↓ Needs download badge shown in the panel
// title row, distinct from the verbose statusLabel used in the Overview rows.
func statusBadge(inst *core.Instance) string {
	if inst.IsFullyDownloaded {
		return lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphPointer + " Ready")
	}
	return lipgloss.NewStyle().Foreground(Active.Warning).Render("↓ Needs download")
}

// playControls renders the launch affordance shown at the top of the Overview
// tab body:
//
//	▸ Play  Enter      Play offline  o
//
// It is intentionally understated (no filled pill) so the Overview reads as a
// calm card: a faint marker, the action in plain bold text, and muted keys. The
// primary label switches to "Login & play" when launching routes through
// sign-in, mirroring buildFooter / noOnlineSession. Enter (from the list, or on
// the focused Overview tab) launches via launchSelected; the offline key is the
// lowercase `o` bound in defaultHomeKeyMap / shown in buildFooter.
func (m *HomeModel) playControls() string {
	action := "Play"
	if m.noOnlineSession() {
		action = "Login & play"
	}
	mark := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(GlyphPointer + " ")
	label := lipgloss.NewStyle().Bold(true).Foreground(Active.Text).Render(action)
	playKey := lipgloss.NewStyle().Foreground(Active.TextMuted).Render("  Enter")
	gap := "      "
	offline := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Play offline")
	offlineKey := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(" o")
	return mark + label + playKey + gap + offline + offlineKey
}

// tabStrip renders the facet selector under the pinned Play header, e.g.
//
//	Overview   Mods   Resource packs
//
// with the active tab accented/underlined and the rest muted. Navigation is via
// the spatial ←/→ model (see the Update key switch), so the strip carries no
// chevrons or number prefixes of its own.
func (m *HomeModel) tabStrip() string {
	active := lipgloss.NewStyle().Bold(true).Underline(true).Foreground(Active.Primary)
	inactive := lipgloss.NewStyle().Foreground(Active.TextMuted)
	sep := lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render("   ")

	parts := make([]string, 0, homeTabCount)
	for i := homeTab(0); i < homeTabCount; i++ {
		if i == m.activeTab {
			parts = append(parts, active.Render(homeTabTitles[i]))
		} else {
			parts = append(parts, inactive.Render(homeTabTitles[i]))
		}
	}
	return strings.Join(parts, sep)
}

// detailBody renders the rich detail panel body for the selected instance: the
// pinned Play header, the tab strip, a rule, then the active facet's read-only
// body. innerW is the panel's inner content width (used to truncate values and
// size rules).
func (m *HomeModel) detailBody(inst *core.Instance, innerW, colH int) string {
	if innerW < 8 {
		innerW = 8
	}

	// Top zone (pinned): the cache-status badge right-aligned on its own line
	// (instance name lives in the border title), the tab strip, then a divider.
	// The Play action lives in the Overview tab body now (playControls), not
	// pinned here.
	top := lipgloss.JoinVertical(lipgloss.Left,
		StatusBar("", statusBadge(inst), innerW),
		m.tabStrip(),
		Rule(innerW),
		"",
	)
	// Bottom zone (pinned to the panel floor): a divider + a context line for the
	// active tab (counts / hints). Anchoring here is what uses the vertical space.
	footer := lipgloss.JoinVertical(lipgloss.Left,
		Rule(innerW),
		m.tabFooter(inst, innerW),
	)

	// The body fills everything between the two pinned zones.
	bodyH := colH - lipgloss.Height(top) - lipgloss.Height(footer)
	if bodyH < 1 {
		bodyH = 1
	}

	var body string
	switch m.activeTab {
	case tabMods:
		body = m.tabModsBody(inst, innerW, bodyH)
	case tabResourcePacks:
		body = m.tabResourcePacksBody(inst, innerW, bodyH)
	default:
		body = m.tabOverviewBody(inst, innerW, bodyH)
	}
	body = lipgloss.NewStyle().MaxHeight(bodyH).Render(body)
	body = lipgloss.PlaceVertical(bodyH, lipgloss.Top, body)

	// Clamp to innerW so no over-wide line (e.g. the tab strip or a long
	// header/row) wraps inside the Panel and inflates the column height.
	return lipgloss.NewStyle().MaxWidth(innerW).Render(
		lipgloss.JoinVertical(lipgloss.Left, top, body, footer))
}

// tabOverviewBody renders a player's at-a-glance card: the primary Play control
// up top, then three labeled sections so the pane reads as structured rather
// than a loose cluster:
//
//	▸ Play  Enter      Play offline  o
//
//	▸ Identity   version · loader (+ cache status when the pane is tall)
//	▸ Content    Mods / Resource packs / Worlds counts
//	▸ Activity   Playtime / Last played / Recent worlds (up to 3)
//
// The Play control is always shown (it is the tab's primary action — Enter on
// the focused Overview launches it). The rest adapts to the available body
// height so a section header is never left orphaned above a clipped body:
//
//   - bodyH ≥ 14 → full: status line under the hero + a blank between sections.
//   - bodyH ≥ 11 → compact: section headers, no status line or blank rows.
//   - otherwise  → dense: no headers, a packed count line, recency stats — so
//     everything stays visible even in a short split pane.
//
// Dev/config detail (Path, Java) lives in the Edit screen, not here.
func (m *HomeModel) tabOverviewBody(inst *core.Instance, innerW, bodyH int) string {
	const labelW = 16
	body := lipgloss.NewStyle().PaddingLeft(2) // nest section bodies under the ▸ markers
	play := m.playControls()                   // primary action, always at the top

	// Shared pieces, used by every tier. Kept neutral (no accent) so the tab reads
	// as a calm, low-contrast card.
	ver := lipgloss.NewStyle().Bold(true).Foreground(Active.Text).Render(inst.Version)
	ldr := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(loaderDisplay(inst))
	dot := lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render("  ·  ")
	hero := ansi.Truncate(ver+dot+ldr, max(1, innerW-2), "…")

	modN := m.modCount(inst)
	packN := len(m.instanceResourcePacks(inst))
	worlds := m.instanceWorlds(inst)
	worldN := len(worlds)

	lastPlayed := "Never"
	if !inst.LastPlayed.IsZero() {
		lastPlayed = formatRelativeTime(inst.LastPlayed)
	}
	// Recency rows. Worlds is no longer its own tab; the list is sorted
	// newest-first, so the Activity section surfaces the (up to) 3 most-recent
	// worlds — "name · last-modified" — answering "what would I jump back into?"
	// without the full list. The first carries the "Recent worlds" label; the rest
	// are unlabeled continuation rows aligned under it. Shown in every height tier
	// (clipped by the body's MaxHeight if the pane is very short).
	activityRows := []string{
		kvRow("Playtime", formatPlayTime(inst.PlayTime), labelW),
		kvRow("Last played", lastPlayed, labelW),
	}
	for i, w := range worlds {
		if i >= 3 {
			break
		}
		label := ""
		if i == 0 {
			label = "Recent worlds"
		}
		val := w.Name
		if !w.ModTime.IsZero() {
			val += "  ·  " + formatRelativeTime(w.ModTime)
		}
		val = ansi.Truncate(val, max(1, innerW-labelW-2), "…")
		activityRows = append(activityRows, kvRow(label, val, labelW))
	}

	// Dense tier: no section headers, a packed count line, recency stats. Keeps
	// the Play control and every stat visible when the pane is too short for the
	// sectioned layout.
	if bodyH < 11 {
		countLine := strings.Join([]string{
			statChip("Mods", modN), statChip("Packs", packN), statChip("Worlds", worldN),
		}, "   ")
		dense := append([]string{play, "", hero, statusLabel(inst), countLine}, activityRows...)
		return lipgloss.JoinVertical(lipgloss.Left, dense...)
	}

	// Sectioned tiers.
	roomy := bodyH >= 14
	idLines := []string{hero}
	if roomy {
		idLines = append(idLines, statusLabel(inst))
	}
	identity := body.Render(lipgloss.JoinVertical(lipgloss.Left, idLines...))

	content := body.Render(lipgloss.JoinVertical(lipgloss.Left,
		kvRow("Mods", fmt.Sprintf("%d", modN), labelW),
		kvRow("Resource packs", fmt.Sprintf("%d", packN), labelW),
		kvRow("Worlds", fmt.Sprintf("%d", worldN), labelW),
	))
	activity := body.Render(lipgloss.JoinVertical(lipgloss.Left, activityRows...))

	parts := []string{play, "", overviewHeader("Identity", innerW), identity}
	sep := func() {
		if roomy {
			parts = append(parts, "")
		}
	}
	sep()
	parts = append(parts, overviewHeader("Content", innerW), content)
	sep()
	parts = append(parts, overviewHeader("Activity", innerW), activity)
	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}

// overviewHeader is the Overview tab's understated section label — a muted title
// and a faint rule, deliberately without SectionHeader's accent ▸ marker so the
// tab reads as a calm, low-contrast card rather than a series of accented bands.
func overviewHeader(title string, width int) string {
	label := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(title)
	rest := width - lipgloss.Width(label) - 1
	if rest < 1 {
		return label
	}
	return label + " " + Rule(rest)
}

// statChip renders a "Label N" content count for the Overview card's dense
// (short-pane) tier: a muted label and a plain bold count — kept neutral (no
// accent) to match the rest of the subtle Overview.
func statChip(label string, n int) string {
	lbl := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(label + " ")
	val := lipgloss.NewStyle().Bold(true).Foreground(Active.Text).Render(fmt.Sprintf("%d", n))
	return lbl + val
}

// detailRowKind classifies a navigable detail row for activation.
type detailRowKind int

const (
	rowAddMods  detailRowKind = iota // "+ Add mods…" → NavigateToMods (Fabric-gated)
	rowAddPacks                      // "+ Add resource packs…" → NavigateToResourcePacks
	rowEntry                         // a mod / pack / world entry → open folder
)

// detailRow is one selectable row in a list tab. label is the rendered text
// (left side); right is an optional right-aligned annotation (size / mtime).
type detailRow struct {
	kind  detailRowKind
	label string // styling applied at render time based on cursor/kind
	right string
	add   bool // true for the leading action rows (rowAddMods / rowAddPacks)
}

// detailRows returns the navigable rows for the active list tab in display
// order: the leading "+ Add…" action (Mods / Resource packs only) followed by
// the installed entries. Overview and compact mode have no rows. Pure (reads the
// cached facets); safe to call from both View and Update.
func (m *HomeModel) detailRows(inst *core.Instance) []detailRow {
	if inst == nil || !homeTabIsList(m.activeTab) {
		return nil
	}
	switch m.activeTab {
	case tabMods:
		rows := []detailRow{{kind: rowAddMods, label: "Add mods…", add: true}}
		jars, _ := mods.ListInstalledJars(inst)
		for _, j := range jars {
			rows = append(rows, detailRow{kind: rowEntry, label: cleanModName(j.Name), right: humanSize(j.Size)})
		}
		return rows
	case tabResourcePacks:
		rows := []detailRow{{kind: rowAddPacks, label: "Add resource packs…", add: true}}
		for _, p := range m.instanceResourcePacks(inst) {
			r := detailRow{kind: rowEntry, label: p.Name}
			switch {
			case p.FromCart:
				r.right = p.Category // group context; size lives in the merged zip
			case p.IsZip:
				r.right = humanSize(p.Size)
			}
			rows = append(rows, r)
		}
		return rows
	}
	return nil
}

// setActiveTab switches the detail facet and resets the detail cursor to the
// top. Focus is preserved (you can tab through facets while focused on detail).
func (m *HomeModel) setActiveTab(t homeTab) {
	m.activeTab = t
	m.detailCursor = 0
}

// focusDetailPane moves input focus to the detail pane on the current tab. It is
// the rightward step in the dashboard's column sequence (List → Overview → Mods
// → …): entering from the list lands on Overview (the list's companion tab),
// from where → walks through the remaining tabs. Overview has no rows, so the
// cursor simply clamps to 0 there. No-op in compact mode (no detail pane).
func (m *HomeModel) focusDetailPane() {
	if m.compactLayout {
		return // no detail pane to focus in compact mode
	}
	if m.SelectedInstance() == nil {
		return // the "+ New instance" row has no detail pane to focus
	}
	m.paneFocus = focusDetail
	if n := m.detailRowCount(); m.detailCursor >= n {
		m.detailCursor = max(0, n-1)
	}
}

// focusListPane returns input focus to the instance list and restores Overview
// as the list's companion view, so the right pane is always the at-a-glance
// summary while you browse instances. The detail cursor is cleared. This is the
// single "back to the list column" entry point (← from Overview, Esc, Tab).
func (m *HomeModel) focusListPane() {
	m.paneFocus = focusList
	m.activeTab = tabOverview
	m.detailCursor = 0
}

// detailRowCount is the number of navigable rows in the active list tab for the
// current selection (0 for Overview / no selection).
func (m *HomeModel) detailRowCount() int {
	return len(m.detailRows(m.SelectedInstance()))
}

// activateDetailRow runs Enter on the row under the detail cursor: the "+ Add…"
// rows navigate (Mods is Fabric-gated, falling back to the transient banner);
// entry rows open the instance folder. Returns the tea.Cmd to run (or nil).
func (m *HomeModel) activateDetailRow() tea.Cmd {
	inst := m.SelectedInstance()
	if inst == nil {
		return nil
	}
	rows := m.detailRows(inst)
	if m.detailCursor < 0 || m.detailCursor >= len(rows) {
		return nil
	}
	switch rows[m.detailCursor].kind {
	case rowAddMods:
		if !instanceSupportsModsBrowser(inst) {
			m.SetTransientBanner("Mods browser isn't available for this instance.")
			return nil
		}
		return func() tea.Msg { return NavigateToMods{Instance: inst} }
	case rowAddPacks:
		return func() tea.Msg { return NavigateToResourcePacks{Instance: inst} }
	default:
		openInstanceFolder(inst.Path)
		return nil
	}
}

// tabModsBody renders the Mods list (leading "+ Add mods…" row + installed
// jars). Non-Fabric instances get a dimmed hint under the action row.
func (m *HomeModel) tabModsBody(inst *core.Instance, innerW, maxRows int) string {
	rows := m.detailRows(inst)
	if len(rows) <= 1 { // only the "+ Add" row → empty state
		hint := "No mods installed yet."
		if !instanceSupportsModsBrowser(inst) {
			hint = "This loader has no in-app mod browser."
		}
		return m.renderRowList(rows, innerW, maxRows, hint)
	}
	return m.renderRowList(rows, innerW, maxRows, "")
}

// tabResourcePacksBody renders the Resource packs list (leading "+ Add resource
// packs…" row + installed packs with sizes for zips).
func (m *HomeModel) tabResourcePacksBody(inst *core.Instance, innerW, maxRows int) string {
	rows := m.detailRows(inst)
	if len(rows) <= 1 {
		return m.renderRowList(rows, innerW, maxRows, "No resource packs yet.")
	}
	return m.renderRowList(rows, innerW, maxRows, "")
}

// renderRowList renders detailRows with the cursor highlight when the detail
// pane is focused, scrolling a window so the cursor stays visible, capping to
// maxRows with a "…N more" tail. emptyHint (when set) is appended dimmed when
// the list has only its action row (no real entries).
func (m *HomeModel) renderRowList(rows []detailRow, innerW, maxRows int, emptyHint string) string {
	if len(rows) == 0 {
		return ""
	}
	dim := lipgloss.NewStyle().Foreground(Active.TextFaint)
	focused := m.paneFocus == focusDetail

	if maxRows < 1 {
		maxRows = 1
	}
	// A "+ Add…" header gets a blank line beneath it to separate the action from
	// the entries; reserve a row for that blank so the body still fits.
	hasAdd := rows[0].add
	rowCap := maxRows
	if hasAdd && rowCap > 2 {
		rowCap--
	}
	// Window the rows so the cursor stays visible; reserve a line for the tail.
	start, capacity := 0, rowCap
	rest := 0
	if len(rows) > capacity {
		capacity = max(1, rowCap-1)
		if focused && m.detailCursor >= capacity {
			start = m.detailCursor - capacity + 1
		}
		if start+capacity > len(rows) {
			start = len(rows) - capacity
		}
		rest = len(rows) - capacity
	}
	end := min(len(rows), start+capacity)

	lines := make([]string, 0, capacity+2)
	for i := start; i < end; i++ {
		lines = append(lines, m.renderDetailRow(rows[i], i, innerW, focused))
		if hasAdd && i == 0 {
			lines = append(lines, "") // breathing room under the +Add header
		}
	}
	if rest > 0 {
		lines = append(lines, dim.Render(fmt.Sprintf("…and %d more", rest)))
	}
	if emptyHint != "" {
		lines = append(lines, dim.Render(emptyHint))
	}
	return lipgloss.JoinVertical(lipgloss.Left, lines...)
}

// renderDetailRow renders a single detail row. Action ("+ Add…") rows render as
// a distinct ActionButton (a pill that fills when it is the cursor row), so they
// read as buttons rather than list entries. Selectable entry rows get the shared
// accent-bar cue + accent label when they are the focused cursor row; otherwise
// normal text with a muted right annotation.
func (m *HomeModel) renderDetailRow(r detailRow, idx, innerW int, focused bool) string {
	selected := focused && idx == m.detailCursor

	// Action rows are buttons, not list items: the filled pill is their own focus
	// cue, so they skip the selection bar and sit in the same 2-cell-inset gutter
	// as the entry rows below them.
	if r.add {
		return "  " + ActionButton(r.label, selected)
	}

	// A 2-cell marker gutter (shared rowMarker: accent bar when selected) +
	// content + a 1-cell right margin, so rows are inset from the panel border
	// and align with the bubbles list's selected left bar.
	contentW := max(1, innerW-3)

	var left, right string
	if selected {
		left = lipgloss.NewStyle().Foreground(Active.Primary).Bold(true).Render(r.label)
		right = lipgloss.NewStyle().Foreground(Active.Text).Render(r.right)
	} else {
		left = lipgloss.NewStyle().Foreground(Active.Text).Render(r.label)
		right = lipgloss.NewStyle().Foreground(Active.TextMuted).Render(r.right)
	}

	return rowMarker(selected) + twoEndsRow(left, right, contentW)
}

// tabFooter is the pinned context line at the foot of the detail panel — counts
// for list tabs, the action-hint legend for Overview.
func (m *HomeModel) tabFooter(inst *core.Instance, innerW int) string {
	dim := lipgloss.NewStyle().Foreground(Active.TextFaint)
	switch m.activeTab {
	case tabMods:
		n := m.modCount(inst)
		return dim.Render(ansi.Truncate(fmt.Sprintf("%d mod%s · ↵ on +Add to browse", n, plural(n)), innerW, "…"))
	case tabResourcePacks:
		n := len(m.instanceResourcePacks(inst))
		return dim.Render(ansi.Truncate(fmt.Sprintf("%d pack%s · ↵ on +Add to browse", n, plural(n)), innerW, "…"))
	default:
		return dim.Render(ansi.Truncate("m mods · p packs · f folder · e edit · x more", innerW, "…"))
	}
}

// twoEndsRow places left at the start and right flush to width, truncating left
// (both args may be pre-styled; widths are measured ANSI-aware).
func twoEndsRow(left, right string, width int) string {
	rw := lipgloss.Width(right)
	left = ansi.Truncate(left, max(1, width-rw-1), "…")
	gap := width - lipgloss.Width(left) - rw
	if gap < 1 {
		gap = 1
	}
	return left + strings.Repeat(" ", gap) + right
}

// humanSize formats a byte count compactly (e.g. "412 KB", "1.2 MB").
func humanSize(n int64) string {
	switch {
	case n >= 1<<20:
		return fmt.Sprintf("%.1f MB", float64(n)/(1<<20))
	case n >= 1<<10:
		return fmt.Sprintf("%d KB", n/(1<<10))
	default:
		return fmt.Sprintf("%d B", n)
	}
}

// renderColumns assembles the master/detail row (two columns) or, in compact
// mode, the single full-width list panel plus a condensed summary line.
func (m *HomeModel) renderColumns() string {
	inst := m.SelectedInstance()

	if m.compactLayout {
		listW := max(1, m.width) // full terminal width; never wider than the pane
		listPanel := Panel("Instances", m.list.View(), listW, Active.Primary)
		summary := ""
		if inst != nil {
			summary = m.detailSummaryLine(inst, listW-2)
		}
		return lipgloss.JoinVertical(lipgloss.Left, listPanel, summary)
	}

	// The list pane is the interactive column → focused accent border. Its
	// rendered height (sized to the layout budget) is the canonical column height
	// that the detail panel fills top-to-bottom.
	listInner := m.list.View()
	colH := lipgloss.Height(listInner)

	var detailInner string
	switch {
	case inst != nil:
		detailInner = m.detailBody(inst, m.detailPaneW-4, colH)
	case m.addRowSelected():
		detailInner = lipgloss.NewStyle().Foreground(Active.TextMuted).
			Render("Create a new instance.\n\n↵ opens the new-instance wizard.")
	default:
		detailInner = lipgloss.NewStyle().Foreground(Active.TextMuted).Render("No instance selected.")
	}
	// Pad/clip both columns to the same height so the two Panel borders align and
	// neither can push the frame past the terminal height.
	if colH > 0 {
		detailInner = lipgloss.PlaceVertical(colH, lipgloss.Top, lipgloss.NewStyle().MaxHeight(colH).Render(detailInner))
		listInner = lipgloss.PlaceVertical(colH, lipgloss.Top, listInner)
	}

	// The focused pane gets the accent border; the other goes faint, so the
	// border reflects where arrow/Enter input lands.
	listBorder, detailBorder := Active.Primary, Active.BorderFaint
	if m.paneFocus == focusDetail {
		listBorder, detailBorder = Active.BorderFaint, Active.Primary
	}

	listPanel := Panel("Instances", listInner, m.listPaneW, listBorder)

	// Border title is JUST the instance name (truncated to the pane so it never
	// overflows the rounded border). The status badge moved into the body, on the
	// Play-header row — see detailBody.
	detailTitle := "Details"
	if inst != nil {
		// Leave breathing room: Panel reserves border + pad; cap the name so the
		// closing dash always renders.
		detailTitle = ansi.Truncate(inst.Name, max(4, m.detailPaneW-8), "…")
	}
	detailPanel := Panel(detailTitle, detailInner, m.detailPaneW, detailBorder)

	return lipgloss.JoinHorizontal(lipgloss.Top, listPanel, "  ", detailPanel)
}

// detailSummaryLine renders the condensed one-line summary shown in compact mode
// for the selected instance, e.g.
//
//	1.21.4 · Fabric 0.16.10 · 12 mods · 2h ago · ✓ Ready
//
// truncated to width.
func (m *HomeModel) detailSummaryLine(inst *core.Instance, width int) string {
	if inst == nil {
		return ""
	}
	sep := lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render(" · ")

	ver := lipgloss.NewStyle().Foreground(Active.Text).Render(inst.Version)
	ldr := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(loaderDisplay(inst))
	modN := lipgloss.NewStyle().Foreground(Active.TextMuted).
		Render(fmt.Sprintf("%d mods", m.modCount(inst)))

	lastPlayed := "Never"
	if !inst.LastPlayed.IsZero() {
		lastPlayed = formatRelativeTime(inst.LastPlayed)
	}
	played := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(lastPlayed)

	var status string
	if inst.IsFullyDownloaded {
		status = lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDone + " Ready")
	} else {
		status = lipgloss.NewStyle().Foreground(Active.Warning).Render(GlyphPending + " Needs download")
	}

	line := ver + sep + ldr + sep + modN + sep + played + sep + status
	return ansi.Truncate(line, max(8, width), "…")
}
