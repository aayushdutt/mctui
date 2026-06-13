package ui

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/aayushdutt/mctui/internal/mods"
	"github.com/charmbracelet/lipgloss"
)

func (m *ModsModel) libraryBannerBlock(nLocal int) string {
	if m.installedErr != "" {
		return lipgloss.NewStyle().Foreground(Active.Error).Render(m.installedErr)
	}
	if m.modsDialog == modsDialogConfirmRemoveJar {
		box := lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(Active.Border).
			Padding(0, 1).
			MarginBottom(0)
		q := lipgloss.NewStyle().Foreground(Active.Text).
			Render(fmt.Sprintf("Remove %q from this instance?", m.modsDialogJar))
		actions := KeyHints(max(24, m.libraryListW), KeyHint{"y/↵", "confirm"}, KeyHint{"n/⌫", "cancel"})
		return box.Render(lipgloss.JoinVertical(lipgloss.Left, q, actions))
	}
	if m.libraryToast != "" {
		return lipgloss.NewStyle().Foreground(Active.SuccessSoft).Render(m.libraryToast)
	}
	if len(m.installed.Items()) == 0 {
		return lipgloss.NewStyle().Foreground(Active.TextDim).Render("Empty — browse →")
	}
	return lipgloss.NewStyle().Foreground(Active.Border).
		Render(fmt.Sprintf("%d jar(s) · %s", nLocal, filepath.Base(mods.ModsDir(m.inst))))
}

func (m *ModsModel) buildModrinthChrome(status string, statusVisible bool, meta, discoverHint string) string {
	qLabel := lipgloss.NewStyle().Foreground(Active.Border).Render("Query")
	if m.modsFocus == panelQuery {
		qLabel = lipgloss.NewStyle().Bold(true).Foreground(Active.Success).Render("Query")
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

	aux := ""
	if strings.TrimSpace(meta) != "" && strings.TrimSpace(discoverHint) != "" {
		aux = lipgloss.JoinHorizontal(lipgloss.Left,
			lipgloss.NewStyle().Foreground(Active.TextMuted).Render(meta),
			lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render("  ·  "),
			discoverHint,
		)
	} else if strings.TrimSpace(meta) != "" {
		aux = meta
	} else {
		aux = discoverHint
	}

	return lipgloss.JoinVertical(lipgloss.Left,
		queryRow,
		statusBlock,
		aux,
	)
}

// View implements tea.Model.
func (m *ModsModel) View() string {
	if m.blocked {
		brand := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render("Mods")
		divider := lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render(strings.Repeat("─", min(42, max(24, m.width-8))))
		body := lipgloss.JoinVertical(
			lipgloss.Center,
			brand,
			lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Fabric instances only"),
			"",
			divider,
			"",
			lipgloss.NewStyle().Foreground(Active.Text).
				Render("Select a Fabric instance on home, then press [m]."),
			"",
			KeyHints(40, KeyHint{"esc", "back"}),
		)
		return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, body)
	}

	nLocal := len(m.installed.Items())

	contentInnerW := max(24, m.width-8)
	brand := lipgloss.NewStyle().Bold(true).Foreground(Active.AccentSofter).Render("Mods")
	ctxLine := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(
		fmt.Sprintf("%s · Minecraft %s · Fabric · %d installed", m.inst.Name, m.inst.Version, nLocal))
	shellRule := lipgloss.NewStyle().Foreground(Active.BorderFaint).Render(strings.Repeat("─", contentInnerW))
	header := lipgloss.NewStyle().MarginBottom(1).Render(lipgloss.JoinVertical(
		lipgloss.Left, brand, ctxLine, shellRule))

	modrinthStatus := ""
	modrinthStatusVisible := false
	switch {
	case m.installing:
		modrinthStatus = lipgloss.NewStyle().Foreground(Active.Secondary).Render("Downloading selected mod…")
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

	meta := ""
	if !m.searching && m.searchErr == "" && m.lastTotalHits > 0 && len(m.results.Items()) > 0 {
		meta = lipgloss.NewStyle().Foreground(Active.TextMuted).
			Render(fmt.Sprintf("Showing %d of ~%s projects", len(m.results.Items()), formatModHitCount(m.lastTotalHits)))
	}

	discoverHint := ""
	if len(m.results.Items()) > 0 && !m.searching {
		discoverHint = lipgloss.NewStyle().Foreground(Active.TextMuted).Italic(true).
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
	libraryInner := lipgloss.JoinVertical(lipgloss.Left,
		libBanner,
		m.installed.View(),
	)
	rightTop := m.buildModrinthChrome(modrinthStatus, modrinthStatusVisible, meta, discoverHint)
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

	// Titled panels: each pane's title sits in its top border; accent reflects focus.
	libAccent := Active.Border
	if m.modsFocus == panelInstalled {
		libAccent = Active.Success
	}
	browseAccent := Active.BorderFaint
	if m.rightColumnFocused() {
		browseAccent = Active.Success
	}
	libraryView := Panel(m.installed.Title, libraryInner, libW+4, libAccent)
	browseBox := Panel("Discover", rightInner, resW+4, browseAccent)

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

	return lipgloss.Place(m.width, m.height, lipgloss.Left, lipgloss.Top,
		lipgloss.NewStyle().Padding(1, 2).Render(lipgloss.JoinVertical(lipgloss.Left, layout, "", help)))
}
