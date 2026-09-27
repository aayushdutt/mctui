package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// libraryBannerBlock is the one line above the installed list: an error, a
// toast, or blank (the pane title carries the count).
func (m *ModsModel) libraryBannerBlock(width int) string {
	line := ""
	switch {
	case m.installedErr != "":
		line = lipgloss.NewStyle().Foreground(Active.Error).Render(m.installedErr)
	case m.libraryToast != "":
		line = lipgloss.NewStyle().Foreground(Active.SuccessSoft).Render(m.libraryToast)
	}
	if line == "" {
		return ""
	}
	return "  " + ansi.Truncate(line, max(1, width-2), "…")
}

// libraryBody is the installed list, or — with no mods — a short hint in its
// place (instead of the list's bare "No mods.").
func (m *ModsModel) libraryBody(width int) string {
	if len(m.installed.Items()) > 0 || m.installedErr != "" {
		return m.installed.View()
	}
	hint := lipgloss.NewStyle().Foreground(Active.TextMuted).Width(max(1, width-2)).
		Render("No mods yet. Pick one in Discover and press ↵ to install it.")
	return lipgloss.NewStyle().PaddingLeft(2).Render(hint)
}

// buildModrinthChrome is the Discover pane above the results:
//
//	/ Search Modrinth…
//	Popular                         6 of 5,056
//	(blank)
//
// The second line is the heading ([resultsHeading]) unless there's a status to
// show (searching, installing, an error), which takes its place.
func (m *ModsModel) buildModrinthChrome(status string, width int) string {
	m.query.PromptStyle = lipgloss.NewStyle().Foreground(Active.TextMuted)
	if m.modsFocus == panelQuery {
		m.query.PromptStyle = lipgloss.NewStyle().Foreground(Active.Primary)
	}
	line := m.resultsHeading(width)
	if status != "" {
		line = "  " + ansi.Truncate(status, max(1, width-2), "…")
	}
	return lipgloss.JoinVertical(lipgloss.Left, "  "+m.query.View(), line, "")
}

// resultsHeading names what the list shows — "Popular" (empty search) or
// "Results" — with a quiet "6 of 5,056" flush right.
func (m *ModsModel) resultsHeading(width int) string {
	title := "Popular"
	if strings.TrimSpace(m.query.Value()) != "" {
		title = "Results"
	}
	left := "  " + lipgloss.NewStyle().Foreground(Active.TextDim).Render(title)
	if m.lastTotalHits == 0 || len(m.results.Items()) == 0 {
		return left
	}
	count := lipgloss.NewStyle().Foreground(Active.TextMuted).
		Render(fmt.Sprintf("%d of %s", len(m.results.Items()), formatModHitCount(m.lastTotalHits)))
	return StatusBar(left, count, width)
}

// View implements tea.Model.
func (m *ModsModel) View() string {
	base := m.baseView()
	switch {
	case m.help:
		return Overlay(base, HelpCard("Mods", m.helpGroups(), m.width, m.height), m.width, m.height)
	case m.palette.Active:
		return Overlay(base, m.palette.View(m.width, m.height), m.width, m.height)
	case m.modsDialog == modsDialogConfirmRemoveJar:
		return Overlay(base, ConfirmDialog{
			Title:    "Remove mod?",
			Message:  fmt.Sprintf("Remove %s\nfrom this instance?", modDisplayNameOnly(m.modsDialogJar)),
			Warning:  "Deletes " + m.modsDialogJar + " from disk.",
			Confirm:  "Remove",
			Cancel:   "Cancel",
			Kind:     ConfirmDanger,
			FocusYes: m.modsDialogFocusYes,
		}.View(m.width), m.width, m.height)
	}
	return base
}

// baseView renders the screen itself (overlays composite over it in View).
func (m *ModsModel) baseView() string {
	nLocal := len(m.installed.Items())

	ctx := fmt.Sprintf("Minecraft %s · Fabric", m.inst.Version)
	header := lipgloss.NewStyle().MarginBottom(1).Render(
		AppHeader(m.width, lipgloss.NewStyle().Foreground(Active.TextDim).Render(ctx), m.inst.Name, "Mods"))

	var status string
	switch {
	case m.installing:
		status = lipgloss.NewStyle().Foreground(Active.Secondary).Render("Installing mod + dependencies…")
	case m.installErr != "":
		status = lipgloss.NewStyle().Foreground(Active.Error).Render(m.installErr)
	case m.installOK != "":
		status = lipgloss.NewStyle().Foreground(Active.SuccessAccent).Render(m.installOK)
	case m.searching && strings.TrimSpace(m.query.Value()) == "":
		status = lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Loading Modrinth…")
	case m.searching:
		status = lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Searching…")
	case m.searchErr != "":
		status = lipgloss.NewStyle().Foreground(Active.Error).Render(m.searchErr)
	case m.searchNotice != "":
		status = lipgloss.NewStyle().Foreground(Active.Warning).Render(m.searchNotice)
	}

	libW := m.libraryListW
	if libW < 4 {
		libW = 22
	}
	resW := m.resultsListW
	if resW < 4 {
		resW = 28
	}
	// Side by side, the banner sits on the heading's row and both lists start on
	// the same line; stacked, it's one line.
	libraryTop := m.libraryBannerBlock(libW)
	if !m.compactLayout {
		libraryTop = lipgloss.JoinVertical(lipgloss.Left, "", libraryTop, "")
	}
	libraryInner := lipgloss.JoinVertical(lipgloss.Left, libraryTop, m.libraryBody(libW))
	rightInner := lipgloss.JoinVertical(lipgloss.Left,
		m.buildModrinthChrome(status, resW),
		m.results.View(),
	)

	if !m.compactLayout {
		mx := max(lipgloss.Height(libraryInner), lipgloss.Height(rightInner))
		if mx > 0 {
			libraryInner = lipgloss.PlaceVertical(mx, lipgloss.Top, libraryInner)
			rightInner = lipgloss.PlaceVertical(mx, lipgloss.Top, rightInner)
		}
	}

	// The focused pane's title is in the accent color; the count sits flush right.
	count := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(fmt.Sprint(nLocal))
	libraryView := ZoneR("Installed", count, libraryInner, libW, m.modsFocus == panelInstalled)
	browseBox := Zone("Discover", rightInner, resW, m.rightColumnFocused())

	layout := ""
	if m.compactLayout {
		layout = lipgloss.JoinVertical(lipgloss.Left,
			header,
			libraryView,
			"",
			browseBox,
		)
	} else {
		row := lipgloss.JoinHorizontal(lipgloss.Top, libraryView, strings.Repeat(" ", 2), browseBox)
		layout = lipgloss.JoinVertical(lipgloss.Left, header, row)
	}

	helpItems := m.footerHelpItems()
	// A blank line where the footer rule used to be keeps the height budget
	// (footerHelpHeight) while matching every other screen's footer.
	help := lipgloss.JoinVertical(lipgloss.Left, "", modsRenderHelp(helpItems, modsHelpBodyMaxWidth(m.width)))

	// No self-padding: the root app applies the single AppShellStyle inset once
	// (app.View), and SetSize already receives the pre-padded contentSize — a
	// second Padding(1,2) here is the double-inset the M2.1 refactor removed. Bound
	// to the content width so a stray sub-cell from a bubble list can't push a line
	// past the frame (mirrors home's final MaxWidth guard).
	return lipgloss.NewStyle().MaxWidth(m.width).Render(
		lipgloss.Place(m.width, m.height, lipgloss.Left, lipgloss.Top,
			lipgloss.JoinVertical(lipgloss.Left, layout, "", help)))
}
