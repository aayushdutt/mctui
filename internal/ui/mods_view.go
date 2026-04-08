package ui

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/mctui/mctui/internal/mods"
)

func (m *ModsModel) libraryBannerBlock(nLocal int) string {
	if m.installedErr != "" {
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#EF4444")).Render(m.installedErr)
	}
	if m.modsDialog == modsDialogConfirmRemoveJar {
		box := lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("#78716C")).
			Padding(0, 1).
			MarginBottom(0)
		q := lipgloss.NewStyle().Foreground(lipgloss.Color("#FAFAFA")).
			Render(fmt.Sprintf("Remove %q from this instance?", m.modsDialogJar))
		actions := lipgloss.NewStyle().Foreground(lipgloss.Color("#A8A29E")).
			Render("[y] or Enter confirm  ·  [n] or Backspace cancel")
		return box.Render(lipgloss.JoinVertical(lipgloss.Left, q, actions))
	}
	if m.libraryToast != "" {
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#6EE7B7")).Render(m.libraryToast)
	}
	if len(m.installed.Items()) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#71717A")).Render("Empty — browse →")
	}
	return lipgloss.NewStyle().Foreground(lipgloss.Color("#57534E")).
		Render(fmt.Sprintf("%d jar(s) · %s", nLocal, filepath.Base(mods.ModsDir(m.inst))))
}

func (m *ModsModel) buildModrinthChrome(status string, statusVisible bool, meta, discoverHint string) string {
	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#C4B5FD")).Render("Discover")
	sub := lipgloss.NewStyle().Foreground(lipgloss.Color("#52525B")).Render("Modrinth · Fabric mods for this version")
	header := lipgloss.JoinHorizontal(lipgloss.Left, title, "  ", sub)

	qLabel := lipgloss.NewStyle().Foreground(lipgloss.Color("#57534E")).Render("Query")
	if m.modsFocus == panelQuery {
		qLabel = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#10B981")).Render("Query")
	}
	searchBox := m.query.View()
	if m.modsFocus != panelQuery {
		searchBox = lipgloss.NewStyle().Foreground(lipgloss.Color("#71717A")).Render(searchBox)
	}
	queryRow := lipgloss.JoinVertical(lipgloss.Left, qLabel, searchBox)

	statusBlock := ""
	if statusVisible && strings.TrimSpace(status) != "" {
		bar := lipgloss.NewStyle().Foreground(lipgloss.Color("#7C3AED")).Render("┃ ")
		statusBlock = lipgloss.JoinHorizontal(lipgloss.Top, bar, status)
	}

	aux := ""
	if strings.TrimSpace(meta) != "" && strings.TrimSpace(discoverHint) != "" {
		aux = lipgloss.JoinHorizontal(lipgloss.Left,
			lipgloss.NewStyle().Foreground(lipgloss.Color("#52525B")).Render(meta),
			lipgloss.NewStyle().Foreground(lipgloss.Color("#3F3F46")).Render("  ·  "),
			discoverHint,
		)
	} else if strings.TrimSpace(meta) != "" {
		aux = meta
	} else {
		aux = discoverHint
	}

	return lipgloss.JoinVertical(lipgloss.Left,
		header,
		"",
		queryRow,
		statusBlock,
		aux,
	)
}

// View implements tea.Model.
func (m *ModsModel) View() string {
	if m.blocked {
		brand := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#7C3AED")).Render("Mods")
		divider := lipgloss.NewStyle().Foreground(lipgloss.Color("#3F3F46")).Render(strings.Repeat("─", min(42, max(24, m.width-8))))
		body := lipgloss.JoinVertical(
			lipgloss.Center,
			brand,
			lipgloss.NewStyle().Foreground(lipgloss.Color("#A1A1AA")).Render("Fabric instances only"),
			"",
			divider,
			"",
			lipgloss.NewStyle().Foreground(lipgloss.Color("#FAFAFA")).
				Render("Select a Fabric instance on home, then press [m]."),
			"",
			lipgloss.NewStyle().Foreground(lipgloss.Color("#626262")).Render("[esc]  Back"),
		)
		return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, body)
	}

	nLocal := len(m.installed.Items())

	contentInnerW := max(24, m.width-8)
	brand := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#E9D5FF")).Render("Mods")
	ctxLine := lipgloss.NewStyle().Foreground(lipgloss.Color("#A1A1AA")).Render(
		fmt.Sprintf("%s · Minecraft %s · Fabric · %d installed", m.inst.Name, m.inst.Version, nLocal))
	shellRule := lipgloss.NewStyle().Foreground(lipgloss.Color("#27272A")).Render(strings.Repeat("─", contentInnerW))
	header := lipgloss.NewStyle().MarginBottom(1).Render(lipgloss.JoinVertical(
		lipgloss.Left, brand, ctxLine, shellRule))

	modrinthStatus := ""
	modrinthStatusVisible := false
	switch {
	case m.installing:
		modrinthStatus = lipgloss.NewStyle().Foreground(lipgloss.Color("#A78BFA")).Render("Downloading selected mod…")
		modrinthStatusVisible = true
	case m.installErr != "":
		modrinthStatus = lipgloss.NewStyle().Foreground(lipgloss.Color("#EF4444")).Render(m.installErr)
		modrinthStatusVisible = true
	case m.installOK != "":
		modrinthStatus = lipgloss.NewStyle().Foreground(lipgloss.Color("#34D399")).Render(m.installOK)
		modrinthStatusVisible = true
	case m.searching:
		modrinthStatusVisible = true
		if strings.TrimSpace(m.query.Value()) == "" {
			modrinthStatus = lipgloss.NewStyle().Foreground(lipgloss.Color("#A1A1AA")).Render("Loading Modrinth…")
		} else {
			modrinthStatus = lipgloss.NewStyle().Foreground(lipgloss.Color("#A1A1AA")).Render("Searching…")
		}
	case m.searchErr != "":
		modrinthStatus = lipgloss.NewStyle().Foreground(lipgloss.Color("#EF4444")).Render(m.searchErr)
		modrinthStatusVisible = true
	case m.searchNotice != "":
		modrinthStatus = lipgloss.NewStyle().Foreground(lipgloss.Color("#FBBF24")).Render(m.searchNotice)
		modrinthStatusVisible = true
	}

	meta := ""
	if !m.searching && m.searchErr == "" && m.lastTotalHits > 0 && len(m.results.Items()) > 0 {
		meta = lipgloss.NewStyle().Foreground(lipgloss.Color("#52525B")).
			Render(fmt.Sprintf("Showing %d of ~%s projects", len(m.results.Items()), formatModHitCount(m.lastTotalHits)))
	}

	discoverHint := ""
	if len(m.results.Items()) > 0 && !m.searching {
		discoverHint = lipgloss.NewStyle().Foreground(lipgloss.Color("#52525B")).Italic(true).
			Render("● = already installed   ◆ = installing")
	}

	libBanner := m.libraryBannerBlock(nLocal)

	libW := m.libraryListW
	if libW < 4 {
		libW = 22
	}
	resW := m.resultsListW
	if resW < 4 {
		resW = 28
	}
	libHdr := modPanelSectionHeader(m.installed.Title, "#34D399", libW)
	browseHdr := modPanelSectionHeader(m.results.Title, "#A78BFA", resW)

	libraryInner := lipgloss.JoinVertical(lipgloss.Left,
		libBanner,
		libHdr,
		m.installed.View(),
	)
	rightTop := m.buildModrinthChrome(modrinthStatus, modrinthStatusVisible, meta, discoverHint)
	rightInner := lipgloss.JoinVertical(lipgloss.Left,
		rightTop,
		browseHdr,
		m.results.View(),
	)

	if !m.compactLayout {
		hl := lipgloss.Height(libraryInner)
		hr := lipgloss.Height(rightInner)
		mx := max(hl, hr)
		if mx > 0 {
			libraryInner = lipgloss.PlaceVertical(mx, lipgloss.Top, libraryInner)
			rightInner = lipgloss.PlaceVertical(mx, lipgloss.Top, rightInner)
		}
	}

	libraryView := m.panelBorderFocused(panelInstalled).Render(libraryInner)

	rightPane := lipgloss.NewStyle().Padding(0, 1).Border(lipgloss.RoundedBorder())
	if m.rightColumnFocused() {
		rightPane = rightPane.BorderForeground(lipgloss.Color("#10B981"))
	} else {
		rightPane = rightPane.BorderForeground(lipgloss.Color("#27272A"))
	}
	browseBox := rightPane.Render(rightInner)

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
	helpRule := lipgloss.NewStyle().Foreground(lipgloss.Color("#3F3F46")).Render(strings.Repeat("─", min(contentInnerW, 56)))
	help := lipgloss.JoinVertical(lipgloss.Left,
		helpRule,
		lipgloss.NewStyle().Foreground(lipgloss.Color("#626262")).MarginTop(1).Render(buildHelpText(helpItems, m.width-6)))

	return lipgloss.Place(m.width, m.height, lipgloss.Left, lipgloss.Top,
		lipgloss.NewStyle().Padding(1, 2).Render(lipgloss.JoinVertical(lipgloss.Left, layout, "", help)))
}
