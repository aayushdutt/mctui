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
	mark := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(GlyphPointer + " ")
	playKey := lipgloss.NewStyle().Foreground(Active.TextMuted).Render("  Enter")
	if inst := m.SelectedInstance(); inst != nil && inst.ID == m.playingID {
		// Its game is running: Enter reopens the launch screen.
		label := lipgloss.NewStyle().Bold(true).Foreground(Active.Text).Render("View game")
		return mark + label + playKey
	}
	action := "Play"
	if m.noOnlineSession() {
		action = "Login & play"
	}
	label := lipgloss.NewStyle().Bold(true).Foreground(Active.Text).Render(action)
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

// detailRow is one row in a read-only detail preview: a label and an optional
// right-aligned annotation (size / category). The Mods and Resource packs tabs
// are previews of their full manager screens, not managers — Enter (in detail
// focus) opens the manager; the preview itself is not row-navigable.
type detailRow struct {
	label string
	right string
}

// tabPreviewEntries returns the installed entries for the active list tab (Mods
// or Resource packs) in display order. Pure (reads cached facets); safe from View.
func (m *HomeModel) tabPreviewEntries(inst *core.Instance) []detailRow {
	if inst == nil {
		return nil
	}
	switch m.activeTab {
	case tabMods:
		jars, _ := mods.ListInstalledJars(inst)
		rows := make([]detailRow, 0, len(jars))
		for _, j := range jars {
			rows = append(rows, detailRow{label: cleanModName(j.Name), right: humanSize(j.Size)})
		}
		return rows
	case tabResourcePacks:
		packs := m.instanceResourcePacks(inst)
		rows := make([]detailRow, 0, len(packs))
		for _, p := range packs {
			r := detailRow{label: p.Name}
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

// setActiveTab switches the detail facet. Focus is preserved (you can tab through
// facets while focused on the detail pane).
func (m *HomeModel) setActiveTab(t homeTab) {
	m.activeTab = t
}

// focusDetailPane moves input focus to the detail pane on the current tab — the
// rightward step in the column sequence (List → Overview → Mods → …). No-op in
// compact mode or on the "+ New instance" row (no detail pane to focus).
func (m *HomeModel) focusDetailPane() {
	if m.compactLayout {
		return
	}
	if m.SelectedInstance() == nil {
		return
	}
	m.paneFocus = focusDetail
}

// focusListPane returns input focus to the instance list and restores Overview as
// the list's companion view. Single "back to the list column" entry point (← from
// Overview, Esc, Tab).
func (m *HomeModel) focusListPane() {
	m.paneFocus = focusList
	m.activeTab = tabOverview
}

// activateDetailTab runs Enter while the detail pane is focused. Overview
// launches the instance (its Play control is the tab's action); the Mods /
// Resource packs previews open their full manager screen (Mods is Fabric-gated,
// falling back to the transient banner). Returns the tea.Cmd to run (or nil).
func (m *HomeModel) activateDetailTab() tea.Cmd {
	inst := m.SelectedInstance()
	if inst == nil {
		return nil
	}
	switch m.activeTab {
	case tabMods:
		if ok, reason := CanBrowseMods(inst); !ok {
			return m.SetTransientBanner(reason)
		}
		return func() tea.Msg { return NavigateToMods{Instance: inst} }
	case tabResourcePacks:
		return func() tea.Msg { return NavigateToResourcePacks{Instance: inst} }
	default:
		return m.launchSelected(false)
	}
}

// tabModsBody renders the read-only Mods preview: the top installed jars, a
// "…N more" tail, or an empty-state hint. Enter (in detail focus) opens the full
// mods manager — see activateDetailTab.
func (m *HomeModel) tabModsBody(inst *core.Instance, innerW, maxRows int) string {
	empty := "No mods installed yet."
	if !instanceSupportsModsBrowser(inst) {
		empty = "This loader has no in-app mod browser."
	}
	return renderPreview(m.tabPreviewEntries(inst), innerW, maxRows, empty)
}

// tabResourcePacksBody renders the read-only Resource packs preview.
func (m *HomeModel) tabResourcePacksBody(inst *core.Instance, innerW, maxRows int) string {
	return renderPreview(m.tabPreviewEntries(inst), innerW, maxRows, "No resource packs yet.")
}

// renderPreview lays out a read-only entry preview: up to maxRows entries with a
// "…N more" tail when they overflow, or a dim empty-state hint. There is no
// cursor — the tabs are previews, not managers (Enter opens the real manager).
func renderPreview(entries []detailRow, innerW, maxRows int, emptyHint string) string {
	dim := lipgloss.NewStyle().Foreground(Active.TextFaint)
	if maxRows < 1 {
		maxRows = 1
	}
	if len(entries) == 0 {
		return dim.Render(emptyHint)
	}
	shown := entries
	rest := 0
	if len(entries) > maxRows {
		shown = entries[:max(1, maxRows-1)] // reserve a row for the "…N more" tail
		rest = len(entries) - len(shown)
	}
	lines := make([]string, 0, len(shown)+1)
	for _, e := range shown {
		lines = append(lines, renderPreviewRow(e, innerW))
	}
	if rest > 0 {
		lines = append(lines, dim.Render(fmt.Sprintf("…and %d more", rest)))
	}
	return lipgloss.JoinVertical(lipgloss.Left, lines...)
}

// renderPreviewRow renders one preview entry: a 2-cell inset, the name, and a
// muted right annotation (size / category). Read-only — no selection cue.
func renderPreviewRow(r detailRow, innerW int) string {
	contentW := max(1, innerW-3)
	left := lipgloss.NewStyle().Foreground(Active.Text).Render(r.label)
	right := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(r.right)
	return "  " + twoEndsRow(left, right, contentW)
}

// tabFooter is the pinned context line at the foot of the detail panel — counts
// for list tabs, the action-hint legend for Overview.
func (m *HomeModel) tabFooter(inst *core.Instance, innerW int) string {
	dim := lipgloss.NewStyle().Foreground(Active.TextFaint)
	switch m.activeTab {
	case tabMods:
		n := m.modCount(inst)
		return dim.Render(ansi.Truncate(fmt.Sprintf("%d mod%s · ↵ open manager", n, plural(n)), innerW, "…"))
	case tabResourcePacks:
		n := len(m.instanceResourcePacks(inst))
		return dim.Render(ansi.Truncate(fmt.Sprintf("%d pack%s · ↵ open manager", n, plural(n)), innerW, "…"))
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
		listPanel := Zone("Instances", m.list.View(), listW, true)
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
		detailInner = m.detailBody(inst, m.detailPaneW, colH)
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

	// The focused pane's Zone title gets the accent ┃ bar; the other stays muted,
	// so the heavy stroke marks where arrow/Enter input lands.
	listPanel := Zone("Instances", listInner, m.listPaneW, m.paneFocus == focusList)

	// The Zone title is JUST the instance name (truncated to the pane so it never
	// overruns the rule). The status badge lives in the body, on the Play-header
	// row — see detailBody.
	detailTitle := "Details"
	if inst != nil {
		// The Zone title has a 2-cell focus gutter; cap the name so it fits the pane.
		detailTitle = ansi.Truncate(inst.Name, max(4, m.detailPaneW-2), "…")
	}
	detailPanel := Zone(detailTitle, detailInner, m.detailPaneW, m.paneFocus == focusDetail)

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
