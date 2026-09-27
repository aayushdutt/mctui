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

// loaderName returns the display name of a loader id ("fabric" → "Fabric",
// "" → "Vanilla").
func loaderName(loader string) string {
	switch loader {
	case "", "vanilla":
		return "Vanilla"
	case "fabric":
		return "Fabric"
	case "forge":
		return "Forge"
	case "quilt":
		return "Quilt"
	case "neoforge":
		return "NeoForge"
	}
	return loader
}

// loaderDisplay returns a human label for an instance's loader, capitalized, with
// the loader version appended when set (e.g. "Fabric 0.16.10").
func loaderDisplay(inst *core.Instance) string {
	name := loaderName(inst.Loader)
	if name == "Vanilla" {
		return name
	}
	if v := inst.LoaderVer; v != "" {
		return name + " " + v
	}
	return name
}

// kvRow renders one detail row: a fixed-width muted label column followed by a
// foreground value. labelW right-pads the label so values align in a column.
func kvRow(label, value string, labelW int) string {
	lbl := lipgloss.NewStyle().Foreground(Active.TextDim).Width(labelW).Render(label)
	val := lipgloss.NewStyle().Foreground(Active.Text).Render(value)
	return lipgloss.JoinHorizontal(lipgloss.Top, lbl, val)
}

// statusBadge is the selected instance's state, shown flush right on the detail
// pane's title line: "● Playing" / "● Ready" (green) or "↓ Not downloaded yet"
// (dim — it's not a problem, the first launch downloads it).
func (m *HomeModel) statusBadge(inst *core.Instance) string {
	switch {
	case inst.ID == m.playingID:
		return lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDot + " Playing")
	case inst.IsFullyDownloaded:
		return lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDot + " Ready")
	default:
		return lipgloss.NewStyle().Foreground(Active.TextDim).Render("↓ Not downloaded yet")
	}
}

// playControls renders the pinned launch block at the top of the detail pane:
//
//	Play            ↵
//	Play offline    o
//
// The primary action is bright and bold, the keys dim and aligned in one
// column. It reads "Sign in & play" when launching routes
// through sign-in (mirroring buildFooter / noOnlineSession) and "View game"
// while the instance is running (Enter reopens its launch screen).
func (m *HomeModel) playControls(inst *core.Instance) string {
	key := lipgloss.NewStyle().Foreground(Active.TextMuted)
	primary := lipgloss.NewStyle().Bold(true).Foreground(Active.TextStrong)
	row := func(label, k string) string {
		return label + strings.Repeat(" ", max(1, 16-lipgloss.Width(label))) + key.Render(k)
	}
	if inst.ID == m.playingID {
		return row(primary.Render("View game"), "↵")
	}
	action := "Play"
	if m.noOnlineSession() {
		action = "Sign in & play"
	}
	offline := lipgloss.NewStyle().Foreground(Active.Text).Render("Play offline")
	return lipgloss.JoinVertical(lipgloss.Left,
		row(primary.Render(action), "↵"),
		row(offline, "o"),
	)
}

// tabStrip renders the facet selector with each list tab's count, e.g.
//
//	Overview   Mods 4   Resource packs 0
//
// The active tab is bright, bold, and underlined (accent while the detail pane
// has focus); the rest are dim with their counts. Navigation is ←/→.
func (m *HomeModel) tabStrip(inst *core.Instance) string {
	activeColor := Active.TextStrong
	if m.paneFocus == focusDetail {
		activeColor = Active.Primary
	}
	active := lipgloss.NewStyle().Bold(true).Underline(true).Foreground(activeColor)
	inactive := lipgloss.NewStyle().Foreground(Active.TextDim)
	count := lipgloss.NewStyle().Foreground(Active.TextMuted)

	parts := make([]string, 0, homeTabCount)
	for i := homeTab(0); i < homeTabCount; i++ {
		label := homeTabTitles[i]
		n := ""
		switch i {
		case tabMods:
			n = fmt.Sprintf(" %d", m.modCount(inst))
		case tabResourcePacks:
			n = fmt.Sprintf(" %d", len(m.instanceResourcePacks(inst)))
		}
		if i == m.activeTab {
			parts = append(parts, active.Render(label)+count.Render(n))
		} else {
			parts = append(parts, inactive.Render(label)+count.Render(n))
		}
	}
	return strings.Join(parts, "   ")
}

// detailBody renders the detail pane under its title: a dim identity line, the
// pinned Play block, the tab strip, then the active tab's read-only body.
// innerW is the pane width; colH the column height to fill.
func (m *HomeModel) detailBody(inst *core.Instance, innerW, colH int) string {
	if innerW < 8 {
		innerW = 8
	}
	dim := lipgloss.NewStyle().Foreground(Active.TextDim)
	identity := "Minecraft " + inst.Version + "  ·  " + loaderDisplay(inst)
	top := lipgloss.JoinVertical(lipgloss.Left,
		"",
		dim.Render(ansi.Truncate(identity, innerW, "…")),
		"",
		m.playControls(inst),
		"",
		m.tabStrip(inst),
		"",
	)

	bodyH := max(1, colH-lipgloss.Height(top))
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

	// Clamp to innerW so no over-wide line wraps and inflates the column height.
	return lipgloss.NewStyle().MaxWidth(innerW).Render(lipgloss.JoinVertical(lipgloss.Left, top, body))
}

// tabOverviewBody renders the at-a-glance rows under the tabs:
//
//	Last played    12h ago
//	Playtime       9h 40m
//	Worlds         New World · just now
//	               Hardcore Run · 3d ago
//
// Labels dim, values bright, no section headers or rules — just aligned rows.
// Recent worlds (newest first) fill whatever height is left, up to three.
func (m *HomeModel) tabOverviewBody(inst *core.Instance, innerW, bodyH int) string {
	const labelW = 15
	lastPlayed := "Never"
	if !inst.LastPlayed.IsZero() {
		lastPlayed = formatRelativeTime(inst.LastPlayed)
	}
	rows := []string{
		kvRow("Last played", lastPlayed, labelW),
		kvRow("Playtime", formatPlayTime(inst.PlayTime), labelW),
	}
	worlds := m.instanceWorlds(inst)
	if len(worlds) == 0 {
		rows = append(rows, kvRow("Worlds", "None yet", labelW))
	}
	for i, w := range worlds {
		if i >= 3 || len(rows) >= bodyH {
			break
		}
		label := ""
		if i == 0 {
			label = "Worlds"
		}
		val := w.Name
		if !w.ModTime.IsZero() {
			val += "  ·  " + formatRelativeTime(w.ModTime)
		}
		rows = append(rows, kvRow(label, ansi.Truncate(val, max(1, innerW-labelW), "…"), labelW))
	}
	return lipgloss.JoinVertical(lipgloss.Left, rows...)
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
	m.delegate.unfocused = true
}

// focusListPane returns input focus to the instance list and restores Overview as
// the list's companion view. Single "back to the list column" entry point (← from
// Overview, Esc, Tab).
func (m *HomeModel) focusListPane() {
	m.paneFocus = focusList
	m.delegate.unfocused = false
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

// instanceCount is the Instances pane title's right side: the count, dim.
func (m *HomeModel) instanceCount() string {
	return lipgloss.NewStyle().Foreground(Active.TextMuted).Render(fmt.Sprintf("%d", len(m.instances)))
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
		listPanel := ZoneR("Instances", m.instanceCount(), m.list.View(), listW, true)
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
		detailInner = lipgloss.NewStyle().Foreground(Active.TextDim).
			Render("\nCreate an instance: pick a Minecraft version,\na loader, and a name.\n\n") +
			KeyHints(m.detailPaneW, KeyHint{"↵", "new instance"})
	default:
		detailInner = lipgloss.NewStyle().Foreground(Active.TextMuted).Render("No instance selected.")
	}
	// Pad/clip both columns to the same height so the two Panel borders align and
	// neither can push the frame past the terminal height.
	if colH > 0 {
		detailInner = lipgloss.PlaceVertical(colH, lipgloss.Top, lipgloss.NewStyle().MaxHeight(colH).Render(detailInner))
		listInner = lipgloss.PlaceVertical(colH, lipgloss.Top, listInner)
	}

	// The focused pane's title is in the accent; the other stays muted.
	listPanel := ZoneR("Instances", m.instanceCount(), listInner, m.listPaneW, m.paneFocus == focusList)

	// The detail title is the instance's name, with its status flush right.
	detailTitle, status := "", ""
	if inst != nil {
		detailTitle, status = inst.Name, m.statusBadge(inst)
	}
	detailPanel := ZoneR(detailTitle, status, detailInner, m.detailPaneW, m.paneFocus == focusDetail)

	return lipgloss.JoinHorizontal(lipgloss.Top, listPanel, "  ", detailPanel)
}

// detailSummaryLine renders the condensed one-line summary shown in compact mode
// for the selected instance, e.g.
//
//	● Ready · 1.21.4 · Fabric 0.16.10 · 12 mods · 2h ago
//
// truncated to width.
func (m *HomeModel) detailSummaryLine(inst *core.Instance, width int) string {
	if inst == nil {
		return ""
	}
	sep := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(" · ")

	ver := lipgloss.NewStyle().Foreground(Active.Text).Render(inst.Version)
	ldr := lipgloss.NewStyle().Foreground(Active.TextDim).Render(loaderDisplay(inst))
	modN := lipgloss.NewStyle().Foreground(Active.TextDim).
		Render(fmt.Sprintf("%d mods", m.modCount(inst)))

	lastPlayed := "Never"
	if !inst.LastPlayed.IsZero() {
		lastPlayed = formatRelativeTime(inst.LastPlayed)
	}
	played := lipgloss.NewStyle().Foreground(Active.TextDim).Render(lastPlayed)

	line := m.statusBadge(inst) + sep + ver + sep + ldr + sep + modN + sep + played
	return ansi.Truncate(line, max(8, width), "…")
}
