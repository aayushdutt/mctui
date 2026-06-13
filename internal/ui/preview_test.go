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

	// Home — with instances
	home2 := NewHomeModel()
	home2.SetSize(w, h)
	home2.SetInstances([]*core.Instance{
		{Name: "Survival World", Version: "1.21.4", Loader: "fabric", LastPlayed: time.Now()},
		{Name: "Fresh Modpack", Version: "1.20.1", Loader: "fabric"}, // never played → "new" badge
	}, "")
	banner("HOME · instance list")
	fmt.Println(home2.View())

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
	auth := NewAuthModel(os.TempDir(), config.DefaultMSAClientID, core.NewAccountManager(os.TempDir()))
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

	fmt.Println(strings.Repeat("─", w))
}
