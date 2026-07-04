package ui

import (
	"fmt"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
)

// TestUIPreview is a developer tool, not an assertion: it prints each screen so
// you can eyeball the aesthetics in color. It is skipped unless UI_PREVIEW=1.
//
//	UI_PREVIEW=1 go test ./internal/ui -run UIPreview -v
//
// Optionally set UI_PREVIEW_THEME=gruvbox (or any theme name) to preview a theme.
func TestUIPreview(t *testing.T) {
	if os.Getenv("UI_PREVIEW") == "" {
		t.Skip("set UI_PREVIEW=1 to render the screen previews")
	}
	// Force color even though `go test` output is not a TTY.
	lipgloss.SetColorProfile(termenv.TrueColor)

	theme := os.Getenv("UI_PREVIEW_THEME")
	if theme == "" {
		theme = "dark"
	}
	Apply(theme)

	const w, h = 88, 26
	banner := func(name string) {
		fmt.Printf("\n\033[7m  %-40s theme=%-12s \033[0m\n\n", name, theme)
	}

	// Home — empty state
	home := NewHomeModel()
	home.SetSize(w, h)
	home.SetInstances(nil, "")
	banner("HOME · empty state")
	fmt.Println(home.View())

	// Home — with instances (two-column master/detail at width >= 84)
	sampleInstances := []*core.Instance{
		{
			Name: "Survival World", Version: "1.21.4", Loader: "fabric", LoaderVer: "0.16.10",
			Path: "/Users/dev/.mctui/instances/survival-world", LastPlayed: time.Now().Add(-2 * time.Hour),
			CreatedAt: time.Now().Add(-30 * 24 * time.Hour), IsFullyDownloaded: true,
		},
		{
			Name: "Fresh Modpack", Version: "1.20.1", Loader: "fabric",
			Path: "/Users/dev/.mctui/instances/fresh-modpack", CreatedAt: time.Now(),
		}, // never played → "new" badge
		{
			Name: "Vanilla 1.21", Version: "1.21", Loader: "vanilla",
			Path: "/Users/dev/.mctui/instances/vanilla-1-21", LastPlayed: time.Now().Add(-3 * 24 * time.Hour),
			CreatedAt: time.Now().Add(-10 * 24 * time.Hour), IsFullyDownloaded: true,
		},
	}
	home2 := NewHomeModel()
	home2.SetSize(w, h)
	home2.SetInstances(sampleInstances, "")
	banner("HOME · instance list")
	fmt.Println(home2.View())

	// Home — each detail facet tab (Overview/Mods/Resource packs) so the
	// tab strip and bodies are eyeballable. Seed saves/, mods/, and resourcepacks/
	// so the list tabs have real content (saves/ feeds the Overview worlds line).
	{
		tabInstDir := t.TempDir()
		saves := tabInstDir + "/.minecraft/saves"
		_ = os.MkdirAll(saves+"/New World", 0o755)
		_ = os.MkdirAll(saves+"/Hardcore Run", 0o755)
		modsDir := tabInstDir + "/.minecraft/mods"
		_ = os.MkdirAll(modsDir, 0o755)
		for _, n := range []string{"sodium-fabric-0.5.jar", "lithium.jar", "fabric-api.jar"} {
			_ = os.WriteFile(modsDir+"/"+n, []byte("x"), 0o644)
		}
		rpDir := tabInstDir + "/.minecraft/resourcepacks"
		_ = os.MkdirAll(rpDir, 0o755)
		_ = os.WriteFile(rpDir+"/Faithful-32x.zip", make([]byte, 412*1024), 0o644)
		_ = os.WriteFile(rpDir+"/VanillaTweaks.zip", make([]byte, 84*1024), 0o644)
		_ = os.MkdirAll(rpDir+"/UnzippedPack", 0o755)
		tabInst := []*core.Instance{{
			Name: "Survival World", Version: "1.21.4", Loader: "fabric", LoaderVer: "0.16.10",
			Path: tabInstDir, LastPlayed: time.Now().Add(-2 * time.Hour),
			CreatedAt: time.Now().Add(-30 * 24 * time.Hour), IsFullyDownloaded: true,
			PlayTime: 4*3600 + 12*60,
		}}
		for _, tc := range []struct {
			tab   homeTab
			label string
		}{
			{tabOverview, "Overview"}, {tabMods, "Mods"},
			{tabResourcePacks, "Resource packs"},
		} {
			ht := NewHomeModel()
			ht.SetInstances(tabInst, "")
			ht.activeTab = tc.tab
			ht.SetSize(w, h)
			banner("HOME · tab " + tc.label)
			fmt.Println(ht.View())
		}

		// Home — detail pane FOCUSED on the Resource packs tab: accent border on
		// the right column, a cursor highlight on the "+ Add resource packs…" row.
		hf := NewHomeModel()
		hf.SetInstances(tabInst, "")
		hf.SetSize(w, h)
		hf.activeTab = tabResourcePacks
		hf.focusDetailPane()
		hf.detailCursor = 0 // on the "+ Add" row
		banner("HOME · detail FOCUSED (Resource packs, + Add)")
		fmt.Println(hf.View())
	}

	// Home — narrow: single column + condensed summary line (compact fallback).
	home3 := NewHomeModel()
	home3.SetSize(72, h)
	home3.SetInstances(sampleInstances, "")
	banner("HOME · narrow")
	fmt.Println(home3.View())

	// Home — actions popup (x) open over the instance list.
	homeAct := NewHomeModel()
	homeAct.SetInstances(sampleInstances, "")
	homeAct.SetSize(w, h)
	homeAct.openActionsPopup(homeAct.SelectedInstance())
	banner("HOME · actions popup (x)")
	fmt.Println(homeAct.View())

	// Home — command palette (:) open with a query typed.
	homePal := NewHomeModel()
	homePal.SetInstances(sampleInstances, "")
	homePal.SetSize(w, h)
	homePal.openPalette()
	homePal.paletteInput.SetValue("va")
	homePal.rebuildPaletteEntries()
	banner("HOME · command palette (:)")
	fmt.Println(homePal.View())

	// New-instance wizard (breadcrumb + panel)
	wiz := NewWizardModel(false)
	wiz.SetSize(w, h)
	banner("WIZARD · step 1")
	fmt.Println(wiz.View())

	// Launch
	launch := NewLaunchModel(&core.Instance{Name: "Survival World", Version: "1.21.4", Loader: "fabric"}, config.DefaultConfig())
	launch.SetSize(w, h)
	banner("LAUNCH")
	fmt.Println(launch.View())

	// Auth — device-code card (white-box: set the state directly)
	auth := NewAuthModel(os.TempDir(), config.DefaultMSAClientID, core.NewAccountManager(os.TempDir()), nil)
	auth.width, auth.height = w, h
	auth.state = AuthStateWaitingForUser
	auth.deviceCode = &api.DeviceCodeResponse{UserCode: "ABCD-EFGH", VerificationURI: "https://microsoft.com/link"}
	banner("AUTH · device code")
	fmt.Println(auth.View())

	// Settings (theme picker)
	settings := NewSettingsModel(config.DefaultConfig())
	settings.SetSize(w, h)
	banner("SETTINGS")
	fmt.Println(settings.View())

	// Edit instance (per-instance overrides)
	edit := NewEditInstanceModel(&core.Instance{
		Name: "Survival World", Version: "1.21.4", Loader: "fabric", LoaderVer: "0.16.10",
		Path: "/Users/dev/.mctui/instances/survival-world", JavaPath: "",
		JVMArgs: []string{"-Xmx4G", "-XX:+UseG1GC"},
	})
	edit.SetSize(w, h)
	banner("EDIT INSTANCE")
	fmt.Println(edit.View())

	fmt.Println(strings.Repeat("─", w))
}
