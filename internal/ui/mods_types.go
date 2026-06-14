package ui

import (
	"fmt"
	"strings"

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
	// Leading swatch: project color for identity, or status color when the row
	// is installed/installing (preserves the at-a-glance status cue on line 1).
	var sc lipgloss.Color
	switch i.rowNote {
	case modRowInstalled:
		sc = Active.SuccessAccent
	case modRowInstalling:
		sc = Active.Warning
	default:
		sc = modSwatchColor(i.hit.Color)
	}
	swatch := lipgloss.NewStyle().Foreground(sc).Render(GlyphDot + " ")
	return swatch + lipgloss.NewStyle().Foreground(Active.TextStrong).Render(i.hit.Title)
}

func (i modListItem) Description() string {
	sep := lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render("  ·  ")
	meta := lipgloss.NewStyle().Foreground(Active.Secondary).Render(api.FormatDownloads(i.hit.Downloads) + " ↓")
	if cats := topCats(i.hit.DisplayCats, 2); cats != "" {
		meta += sep + lipgloss.NewStyle().Foreground(Active.TextMuted).Render(cats)
	}
	if badge := modCompatBadge(i.hit.ClientSide, i.hit.ServerSide); badge != "" {
		meta += "  " + badge
	}

	switch i.rowNote {
	case modRowInstalled:
		return lipgloss.JoinHorizontal(lipgloss.Left, modStatusPill("installed", Active.SuccessBg, Active.SuccessFaint), " ", meta)
	case modRowInstalling:
		return lipgloss.JoinHorizontal(lipgloss.Left, modStatusPill("installing", Active.WarningBg, Active.WarningSoft), " ", meta)
	default:
		return meta
	}
}

// modSwatchColor maps Modrinth's packed RGB project color to a lipgloss color,
// falling back to a dim neutral when absent (color == 0).
func modSwatchColor(c int) lipgloss.Color {
	if c <= 0 {
		return Active.TextDim
	}
	return lipgloss.Color(fmt.Sprintf("#%06X", c&0xFFFFFF))
}

// topCats joins the first n human-readable categories with a middot.
func topCats(cats []string, n int) string {
	if len(cats) > n {
		cats = cats[:n]
	}
	return strings.Join(cats, " · ")
}

// modCompatBadge surfaces a side-specific compatibility note, but only when the
// mod is one-sided (client-only or server-only); the common both-sides case
// shows nothing to avoid row clutter.
func modCompatBadge(client, server string) string {
	clientNeeds := client == "required" || client == "optional"
	serverNeeds := server == "required" || server == "optional"
	var label string
	switch {
	case clientNeeds && server == "unsupported":
		label = "client"
	case serverNeeds && client == "unsupported":
		label = "server"
	default:
		return ""
	}
	return lipgloss.NewStyle().Foreground(Active.TextFaint).Render("[" + label + "]")
}

func modStatusPill(label string, bg, fg lipgloss.Color) string {
	return lipgloss.NewStyle().Background(bg).Foreground(fg).Padding(0, 1).Render(label)
}

func (i modListItem) FilterValue() string { return i.hit.Title + " " + i.hit.Slug }

type modInstalledItem struct {
	jar mods.InstalledJar
}

func (i modInstalledItem) Title() string       { return i.jar.Name }
func (i modInstalledItem) Description() string { return humanize.Bytes(uint64(i.jar.Size)) }
func (i modInstalledItem) FilterValue() string { return i.jar.Name }
