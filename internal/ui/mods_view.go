package ui

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/aayushdutt/mctui/internal/mods"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

func (m *ModsModel) libraryBannerBlock(nLocal int) string {
	if m.installedErr != "" {
		return lipgloss.NewStyle().Foreground(Active.Error).Render(m.installedErr)
	}
	if m.libraryToast != "" {
		return lipgloss.NewStyle().Foreground(Active.SuccessSoft).Render(m.libraryToast)
	}
	if len(m.installed.Items()) == 0 {
		return lipgloss.NewStyle().Foreground(Active.TextDim).Render("No mods — search →")
	}
	return lipgloss.NewStyle().Foreground(Active.Border).
		Render(fmt.Sprintf("%d jar(s) · %s", nLocal, filepath.Base(mods.ModsDir(m.inst))))
}

func (m *ModsModel) buildModrinthChrome(status string, statusVisible bool, meta string) string {
	qLabel := lipgloss.NewStyle().Foreground(Active.Border).Render("Query")
	if m.modsFocus == panelQuery {
		// Focus is the accent (Primary) everywhere — never green.
		qLabel = lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render("Query")
	}
	searchBox := m.query.View()
	if m.modsFocus != panelQuery {
		searchBox = lipgloss.NewStyle().Foreground(Active.TextDim).Render(searchBox)
	}
	queryRow := lipgloss.JoinVertical(lipgloss.Left, qLabel, searchBox)

	statusBlock := ""
	if statusVisible && strings.TrimSpace(status) != "" {
		bar := lipgloss.NewStyle().Foreground(Active.Primary).Render("┃ ")
		statusBlock = lipgloss.JoinHorizontal(lipgloss.Top, bar, status)
	}

	return lipgloss.JoinVertical(lipgloss.Left,
		queryRow,
		statusBlock,
		meta,
	)
}

// View implements tea.Model.
func (m *ModsModel) View() string {
	if m.help {
		return boundToScreen(HelpOverlay("Mods", m.helpGroups(), m.width, m.height), m.width, m.height)
	}
	if m.palette.Active {
		return boundToScreen(m.palette.View(m.width, m.height), m.width, m.height)
	}

	if m.modsDialog == modsDialogConfirmRemoveJar {
		return ConfirmDialog{
			Title:    "Remove mod?",
			Message:  fmt.Sprintf("Remove %q\nfrom this instance?", m.modsDialogJar),
			Warning:  "Deletes the .jar from disk.",
			Confirm:  "Remove",
			Cancel:   "Cancel",
			Kind:     ConfirmDanger,
			FocusYes: m.modsDialogFocusYes,
		}.Render(m.width, m.height)
	}

	nLocal := len(m.installed.Items())

	contentInnerW := max(24, m.width-8)
	ctxLineText := fmt.Sprintf("%s · Minecraft %s · Fabric · %d installed", m.inst.Name, m.inst.Version, nLocal)
	header := lipgloss.NewStyle().MarginBottom(1).Render(lipgloss.JoinVertical(
		lipgloss.Left, ScreenHeader("Mods", ctxLineText), Rule(contentInnerW)))

	modrinthStatus := ""
	modrinthStatusVisible := false
	switch {
	case m.installing:
		modrinthStatus = lipgloss.NewStyle().Foreground(Active.Secondary).Render("Installing mod + dependencies…")
		modrinthStatusVisible = true
	case m.installErr != "":
		modrinthStatus = lipgloss.NewStyle().Foreground(Active.Error).Render(m.installErr)
		modrinthStatusVisible = true
	case m.installOK != "":
		modrinthStatus = lipgloss.NewStyle().Foreground(Active.SuccessAccent).Render(m.installOK)
		modrinthStatusVisible = true
	case m.searching:
		modrinthStatusVisible = true
		if strings.TrimSpace(m.query.Value()) == "" {
			modrinthStatus = lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Loading Modrinth…")
		} else {
			modrinthStatus = lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Searching…")
		}
	case m.searchErr != "":
		modrinthStatus = lipgloss.NewStyle().Foreground(Active.Error).Render(m.searchErr)
		modrinthStatusVisible = true
	case m.searchNotice != "":
		modrinthStatus = lipgloss.NewStyle().Foreground(Active.Warning).Render(m.searchNotice)
		modrinthStatusVisible = true
	}
	// Bound the status line to the browse pane so a long error/notice can't widen
	// the Zone past its column (Zones don't clamp the body the way Panel did).
	if modrinthStatus != "" {
		rw := m.resultsListW
		if rw < 4 {
			rw = 28
		}
		modrinthStatus = ansi.Truncate(modrinthStatus, rw, "…")
	}

	meta := ""
	if !m.searching && m.searchErr == "" && m.lastTotalHits > 0 && len(m.results.Items()) > 0 {
		meta = lipgloss.NewStyle().Foreground(Active.TextMuted).
			Render(fmt.Sprintf("Showing %d of ~%s projects", len(m.results.Items()), formatModHitCount(m.lastTotalHits)))
	}

	// Rows carry their own swatch + installed/installing pills, so no legend.
	libBanner := m.libraryBannerBlock(nLocal)

	libW := m.libraryListW
	if libW < 4 {
		libW = 22
	}
	resW := m.resultsListW
	if resW < 4 {
		resW = 28
	}
	libraryInner := lipgloss.JoinVertical(lipgloss.Left,
		libBanner,
		m.installed.View(),
	)
	rightTop := m.buildModrinthChrome(modrinthStatus, modrinthStatusVisible, meta)
	rightInner := lipgloss.JoinVertical(lipgloss.Left,
		rightTop,
		SectionHeader(m.results.Title, resW),
		m.results.View(),
	)

	if !m.compactLayout {
		mx := max(lipgloss.Height(libraryInner), lipgloss.Height(rightInner))
		if mx > 0 {
			libraryInner = lipgloss.PlaceVertical(mx, lipgloss.Top, libraryInner)
			rightInner = lipgloss.PlaceVertical(mx, lipgloss.Top, rightInner)
		}
	}

	// De-boxed Zones: the focused pane shows a ┃ accent title, the other a muted
	// one, over a dim rule — the ┃ is the single "you are here" stroke (replacing
	// the old ▸ marker + per-pane accent border).
	libraryView := Zone(m.installed.Title, libraryInner, libW, m.modsFocus == panelInstalled)
	browseBox := Zone("Discover", rightInner, resW, m.rightColumnFocused())

	layout := ""
	if m.compactLayout {
		layout = lipgloss.JoinVertical(lipgloss.Left,
			header,
			"",
			libraryView,
			"",
			browseBox,
		)
	} else {
		row := lipgloss.JoinHorizontal(lipgloss.Top, libraryView, strings.Repeat(" ", 2), browseBox)
		layout = lipgloss.JoinVertical(lipgloss.Left, header, "", row)
	}

	helpItems := m.footerHelpItems()
	help := lipgloss.JoinVertical(lipgloss.Left,
		Rule(min(contentInnerW, 56)),
		lipgloss.NewStyle().MarginTop(1).Render(modsRenderHelp(helpItems, m.width-6)))

	// No self-padding: the root app applies the single AppShellStyle inset once
	// (app.View), and SetSize already receives the pre-padded contentSize — a
	// second Padding(1,2) here is the double-inset the M2.1 refactor removed. Bound
	// to the content width so a stray sub-cell from a bubble list can't push a line
	// past the frame (mirrors home's final MaxWidth guard).
	return lipgloss.NewStyle().MaxWidth(m.width).Render(
		lipgloss.Place(m.width, m.height, lipgloss.Left, lipgloss.Top,
			lipgloss.JoinVertical(lipgloss.Left, layout, "", help)))
}
