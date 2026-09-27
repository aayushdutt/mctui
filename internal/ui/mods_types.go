package ui

import (
	"strings"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/mods"
	"github.com/charmbracelet/lipgloss"
	"github.com/dustin/go-humanize"
)

const splitMinWidth = 76

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
	prefix, name, st := i.titleParts()
	return prefix + st.Render(name)
}

// titleParts returns the row's plain title (no prefix) so the shared delegate
// can highlight filter matches safely (see styledTitleItem).
func (i modListItem) titleParts() (string, string, lipgloss.Style) {
	return "", i.hit.Title, lipgloss.NewStyle()
}

// Description is the row's quiet second line:
//
//	✓ Installed · 146.6M downloads · client-side · optimization
//
// install state first (the only color), then downloads, "client-side" /
// "server-side" for one-sided mods, and the first couple of real categories
// (loader names are dropped — every result here is a Fabric mod).
func (i modListItem) Description() string {
	dim := lipgloss.NewStyle().Foreground(Active.TextDim)
	muted := lipgloss.NewStyle().Foreground(Active.TextMuted)
	var parts []string
	switch i.rowNote {
	case modRowInstalled:
		parts = append(parts, lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDone+" Installed"))
	case modRowInstalling:
		parts = append(parts, lipgloss.NewStyle().Foreground(Active.Warning).Render("Installing…"))
	}
	parts = append(parts, dim.Render(api.FormatDownloads(i.hit.Downloads)+" downloads"))
	if side := modSide(i.hit.ClientSide, i.hit.ServerSide); side != "" {
		parts = append(parts, muted.Render(side))
	}
	if cats := modCategories(i.hit.DisplayCats, 2); cats != "" {
		parts = append(parts, muted.Render(cats))
	}
	return strings.Join(parts, muted.Render(" · "))
}

// modLoaderCats are Modrinth categories that just name a loader (Modrinth lists
// a project's loaders among its display categories).
var modLoaderCats = map[string]bool{
	"fabric": true, "quilt": true, "forge": true, "neoforge": true, "babric": true, "bta-babric": true,
	"legacy-fabric": true, "ornithe": true, "rift": true, "liteloader": true, "modloader": true,
	"nilloader": true, "java-agent": true,
}

// modCategories joins the first n categories that aren't loader names.
func modCategories(cats []string, n int) string {
	var out []string
	for _, c := range cats {
		if modLoaderCats[strings.ToLower(c)] {
			continue
		}
		if out = append(out, c); len(out) == n {
			break
		}
	}
	return strings.Join(out, " · ")
}

// modSide names a one-sided mod ("client-side" / "server-side"); mods that run
// on both sides (the common case) get nothing, to keep rows quiet.
func modSide(client, server string) string {
	clientNeeds := client == "required" || client == "optional"
	serverNeeds := server == "required" || server == "optional"
	switch {
	case clientNeeds && server == "unsupported":
		return "client-side"
	case serverNeeds && client == "unsupported":
		return "server-side"
	}
	return ""
}

func (i modListItem) FilterValue() string { return i.hit.Title + " " + i.hit.Slug }

type modInstalledItem struct {
	jar mods.InstalledJar
}

func (i modInstalledItem) Title() string { return modDisplayNameOnly(i.jar.Name) }

// Description is "0.9.1 · 1.2 MB" (the list styles it quiet).
func (i modInstalledItem) Description() string {
	_, version := modDisplayName(i.jar.Name)
	size := humanize.Bytes(uint64(i.jar.Size))
	if version == "" {
		return size
	}
	return version + " · " + size
}
func (i modInstalledItem) FilterValue() string { return i.jar.Name }

// modAcronyms keeps well-known short words upper-case when a name is rebuilt
// from a lower-case file name.
var modAcronyms = map[string]string{"api": "API", "yacl": "YACL", "emf": "EMF", "etf": "ETF", "fps": "FPS", "ui": "UI", "hud": "HUD"}

// modDisplayName turns a mod jar's file name into a readable name and version:
//
//	"sodium-fabric-0.9.1+mc26.2.jar"      → "Sodium", "0.9.1"
//	"fabric-api-0.154.2+26.2.jar"         → "Fabric API", "0.154.2"
//	"ImmediatelyFast-Fabric-1.17.1+26.3"  → "ImmediatelyFast", "1.17.1"
//	"Xaeros_Minimap_24.1.0_Fabric_1.21"   → "Xaeros Minimap", "24.1.0"
//
// The name is the words (split at - _ and spaces) before the first version-like one
// ("0.9.1", "v15.0", "mc1.21.4"), minus trailing loader words; the version is
// the first word from there that starts with a digit, up to any "+" build
// metadata. Names already in mixed case are kept as written.
func modDisplayName(file string) (name, version string) {
	base := strings.TrimSuffix(strings.TrimSuffix(file, ".jar"), ".JAR")
	words := strings.FieldsFunc(base, func(r rune) bool { return r == '-' || r == '_' || r == ' ' })
	k := len(words)
	for i, w := range words {
		if i > 0 && (isVersionWord(w) || isMCVersionWord(w)) {
			k = i
			break
		}
	}
	nameWords := words[:k]
	for len(nameWords) > 1 && modLoaderCats[strings.ToLower(nameWords[len(nameWords)-1])] {
		nameWords = nameWords[:len(nameWords)-1]
	}
	for _, w := range words[k:] {
		if isVersionWord(w) {
			version, _, _ = strings.Cut(strings.TrimPrefix(w, "v"), "+")
			break
		}
	}
	joined := strings.Join(nameWords, " ")
	if joined == strings.ToLower(joined) {
		for i, w := range nameWords {
			if a, ok := modAcronyms[w]; ok {
				nameWords[i] = a
			} else if w != "" {
				nameWords[i] = strings.ToUpper(w[:1]) + w[1:]
			}
		}
		joined = strings.Join(nameWords, " ")
	}
	if strings.TrimSpace(joined) == "" {
		return base, version
	}
	return joined, version
}

// modDisplayNameOnly is [modDisplayName] without the version.
func modDisplayNameOnly(file string) string {
	name, _ := modDisplayName(file)
	return name
}

// isVersionWord reports whether a file-name word is a version: "0.9.1+mc26.2",
// "v15.0.140".
func isVersionWord(w string) bool {
	w = strings.TrimPrefix(w, "v")
	return w != "" && w[0] >= '0' && w[0] <= '9'
}

// isMCVersionWord reports whether a file-name word names the Minecraft version
// ("mc1.21.4"): it ends the mod's name but isn't the mod's version.
func isMCVersionWord(w string) bool {
	return len(w) > 2 && strings.EqualFold(w[:2], "mc") && w[2] >= '0' && w[2] <= '9'
}
