package ui

import (
	"fmt"

	"github.com/charmbracelet/lipgloss"
	"github.com/dustin/go-humanize"
	"github.com/mctui/mctui/internal/api"
	"github.com/mctui/mctui/internal/mods"
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
	t := lipgloss.NewStyle().Foreground(lipgloss.Color("#F4F4F5")).Render(i.hit.Title)
	switch i.rowNote {
	case modRowInstalled:
		dot := lipgloss.NewStyle().Foreground(lipgloss.Color("#34D399")).Render("● ")
		return dot + t
	case modRowInstalling:
		dot := lipgloss.NewStyle().Foreground(lipgloss.Color("#FBBF24")).Render("◆ ")
		return dot + t
	default:
		return i.hit.Title
	}
}

func (i modListItem) Description() string {
	meta := fmt.Sprintf("%s • %s", api.FormatDownloads(i.hit.Downloads), i.hit.Slug)
	metaStyled := lipgloss.NewStyle().Foreground(lipgloss.Color("#71717A")).Render(meta)
	switch i.rowNote {
	case modRowInstalled:
		pill := lipgloss.NewStyle().
			Background(lipgloss.Color("#14532D")).
			Foreground(lipgloss.Color("#A7F3D0")).
			Padding(0, 1).
			Render("installed")
		return lipgloss.JoinHorizontal(lipgloss.Left, pill, " ", metaStyled)
	case modRowInstalling:
		pill := lipgloss.NewStyle().
			Background(lipgloss.Color("#422006")).
			Foreground(lipgloss.Color("#FCD34D")).
			Padding(0, 1).
			Render("installing")
		return lipgloss.JoinHorizontal(lipgloss.Left, pill, " ", metaStyled)
	default:
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#A1A1AA")).Render(meta)
	}
}

func (i modListItem) FilterValue() string { return i.hit.Title + " " + i.hit.Slug }

type modInstalledItem struct {
	jar mods.InstalledJar
}

func (i modInstalledItem) Title() string       { return i.jar.Name }
func (i modInstalledItem) Description() string { return humanize.Bytes(uint64(i.jar.Size)) }
func (i modInstalledItem) FilterValue() string { return i.jar.Name }
