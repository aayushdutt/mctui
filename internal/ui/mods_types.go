package ui

import (
	"fmt"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/mods"
	"github.com/charmbracelet/lipgloss"
	"github.com/dustin/go-humanize"
)

const splitMinWidth = 78

type modsPanel int

const (
	panelInstalled modsPanel = iota
	panelQuery
	panelBrowse
)

type modsDialogKind int

const (
	modsDialogNone modsDialogKind = iota
	modsDialogConfirmRemoveJar
)

type modSearchDueMsg struct{ seq int }
type modSearchStaleMsg struct{}
type modSearchResultMsg struct {
	seq    int
	result *api.SearchResult
	err    error
}

const (
	modRowInstalled  = "✓ Installed"
	modRowInstalling = "Installing…"
)

type modListItem struct {
	hit     api.SearchHit
	rowNote string // modRow* or ""
}

func (i modListItem) Title() string {
	t := lipgloss.NewStyle().Foreground(Active.TextStrong).Render(i.hit.Title)
	switch i.rowNote {
	case modRowInstalled:
		dot := lipgloss.NewStyle().Foreground(Active.SuccessAccent).Render("● ")
		return dot + t
	case modRowInstalling:
		dot := lipgloss.NewStyle().Foreground(Active.Warning).Render("◆ ")
		return dot + t
	default:
		return i.hit.Title
	}
}

func (i modListItem) Description() string {
	meta := fmt.Sprintf("%s • %s", api.FormatDownloads(i.hit.Downloads), i.hit.Slug)
	metaStyled := lipgloss.NewStyle().Foreground(Active.TextDim).Render(meta)
	switch i.rowNote {
	case modRowInstalled:
		pill := lipgloss.NewStyle().
			Background(Active.SuccessBg).
			Foreground(Active.SuccessFaint).
			Padding(0, 1).
			Render("installed")
		return lipgloss.JoinHorizontal(lipgloss.Left, pill, " ", metaStyled)
	case modRowInstalling:
		pill := lipgloss.NewStyle().
			Background(Active.WarningBg).
			Foreground(Active.WarningSoft).
			Padding(0, 1).
			Render("installing")
		return lipgloss.JoinHorizontal(lipgloss.Left, pill, " ", metaStyled)
	default:
		return lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(meta)
	}
}

func (i modListItem) FilterValue() string { return i.hit.Title + " " + i.hit.Slug }

type modInstalledItem struct {
	jar mods.InstalledJar
}

func (i modInstalledItem) Title() string       { return i.jar.Name }
func (i modInstalledItem) Description() string { return humanize.Bytes(uint64(i.jar.Size)) }
func (i modInstalledItem) FilterValue() string { return i.jar.Name }
