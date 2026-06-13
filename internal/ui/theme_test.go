package ui

import (
	"reflect"
	"sort"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

// restoreDark resets Active to the dark theme so a test that swaps the palette
// doesn't leak state into other package tests.
func restoreDark(t *testing.T) {
	t.Helper()
	t.Cleanup(func() { Apply("dark") })
}

// TestThemesCompleteRoles checks that every registered theme fills in every
// color role. A Palette field of type lipgloss.Color must be non-empty, except
// Background (intentionally allowed to be empty so the terminal backdrop shows
// through). The Name string field is skipped too. This catches a theme author
// forgetting a role.
func TestThemesCompleteRoles(t *testing.T) {
	restoreDark(t)

	colorType := reflect.TypeOf(lipgloss.Color(""))

	for _, name := range ThemeNames() {
		if !Apply(name) {
			t.Fatalf("Apply(%q) returned false for a registered theme", name)
		}

		v := reflect.ValueOf(Active)
		typ := v.Type()
		for i := 0; i < typ.NumField(); i++ {
			field := typ.Field(i)

			// Skip the registry-key string field.
			if field.Name == "Name" {
				continue
			}
			// Only check color-role fields.
			if field.Type != colorType {
				continue
			}
			// Background is allowed to be empty (inherits terminal background).
			if field.Name == "Background" {
				continue
			}

			if v.Field(i).Interface().(lipgloss.Color) == "" {
				t.Errorf("theme %q: role %q is empty", name, field.Name)
			}
		}
	}
}

// TestApplyUnknownThemeKeepsActive verifies that an unknown name is a no-op:
// Apply returns false and the active theme is unchanged.
func TestApplyUnknownThemeKeepsActive(t *testing.T) {
	restoreDark(t)

	before := ActiveName()

	if Apply("does-not-exist") {
		t.Error("Apply(\"does-not-exist\") = true, want false")
	}
	if got := ActiveName(); got != before {
		t.Errorf("ActiveName() changed after unknown theme: got %q, want %q", got, before)
	}

	if !Apply("dark") {
		t.Error("Apply(\"dark\") = false, want true")
	}
}

// TestAllExpectedThemesRegistered asserts the exact set of selectable themes and
// that "auto" heads the list (it is the default).
func TestAllExpectedThemesRegistered(t *testing.T) {
	want := []string{"auto", "dark", "light", "gruvbox", "catppuccin", "dracula", "nord"}

	got := ThemeNames()

	if len(got) == 0 || got[0] != AutoTheme {
		t.Errorf("ThemeNames()[0] = %q, want %q (auto must be first)", firstOrEmpty(got), AutoTheme)
	}

	gotSorted := append([]string(nil), got...)
	wantSorted := append([]string(nil), want...)
	sort.Strings(gotSorted)
	sort.Strings(wantSorted)

	if !reflect.DeepEqual(gotSorted, wantSorted) {
		t.Errorf("ThemeNames() set = %v, want %v (order-independent)", got, want)
	}
}

func firstOrEmpty(s []string) string {
	if len(s) == 0 {
		return ""
	}
	return s[0]
}

// TestDarkThemeIsDefault guards the default look: dark must be named "dark" and
// preserve the original pre-theming hexes.
func TestDarkThemeIsDefault(t *testing.T) {
	restoreDark(t)

	if !Apply("dark") {
		t.Fatal("Apply(\"dark\") = false, want true")
	}

	if Active.Name != "dark" {
		t.Errorf("Active.Name = %q, want \"dark\"", Active.Name)
	}

	checks := []struct {
		role string
		got  lipgloss.Color
		want lipgloss.Color
	}{
		{"Primary", Active.Primary, lipgloss.Color("#7C3AED")},
		{"Error", Active.Error, lipgloss.Color("#EF4444")},
		{"Success", Active.Success, lipgloss.Color("#10B981")},
	}
	for _, c := range checks {
		if c.got != c.want {
			t.Errorf("dark theme %s = %q, want %q", c.role, c.got, c.want)
		}
	}
}

// TestAutoThemeResolvesToTerminal verifies AutoTheme picks the dark or light
// palette based on the detected terminal background while keeping the "auto"
// name so the choice round-trips through config and the picker.
func TestAutoThemeResolvesToTerminal(t *testing.T) {
	orig := lipgloss.HasDarkBackground()
	t.Cleanup(func() {
		lipgloss.SetHasDarkBackground(orig)
		Apply("dark")
	})

	lipgloss.SetHasDarkBackground(true)
	if !Apply(AutoTheme) {
		t.Fatal("Apply(AutoTheme) = false, want true")
	}
	if Active.Name != AutoTheme {
		t.Errorf("Active.Name = %q, want %q", Active.Name, AutoTheme)
	}
	if Active.Primary != darkTheme.Primary {
		t.Errorf("auto on dark terminal: Primary = %q, want dark %q", Active.Primary, darkTheme.Primary)
	}

	lipgloss.SetHasDarkBackground(false)
	Apply(AutoTheme)
	if Active.Name != AutoTheme {
		t.Errorf("Active.Name = %q, want %q after light resolve", Active.Name, AutoTheme)
	}
	if Active.Primary != lightTheme.Primary {
		t.Errorf("auto on light terminal: Primary = %q, want light %q", Active.Primary, lightTheme.Primary)
	}
}

// minContrast is WCAG AA for normal-size text.
const minContrast = 4.5

// TestOnColorMeetsContrast verifies OnColor picks a readable foreground for the
// saturated accent backgrounds that paint text (title bars, status pills) in
// every registered theme.
func TestOnColorMeetsContrast(t *testing.T) {
	restoreDark(t)

	for _, name := range themeOrder { // concrete palettes only
		Apply(name)
		bgs := map[string]lipgloss.Color{
			"Primary": Active.Primary,
			"Success": Active.Success,
			"Error":   Active.Error,
		}
		for role, bg := range bgs {
			ratio, ok := ContrastRatio(OnColor(bg), bg)
			if !ok {
				t.Errorf("theme %q: %s = %q is not a parseable hex", name, role, bg)
				continue
			}
			if ratio < minContrast {
				t.Errorf("theme %q: OnColor(%s) contrast %.2f < %.1f", name, role, ratio, minContrast)
			}
		}
	}
}

// TestBadgePairsMeetContrast checks the author-defined fg/bg pairs (the status
// badges) hold AA contrast in every theme — these are the pairs we fully
// control, so a low-contrast one is a theme bug.
func TestBadgePairsMeetContrast(t *testing.T) {
	restoreDark(t)

	for _, name := range themeOrder {
		Apply(name)
		pairs := []struct {
			label  string
			fg, bg lipgloss.Color
		}{
			{"success badge", Active.SuccessFaint, Active.SuccessBg},
			{"warning badge", Active.WarningSoft, Active.WarningBg},
		}
		for _, p := range pairs {
			ratio, ok := ContrastRatio(p.fg, p.bg)
			if !ok {
				t.Errorf("theme %q: %s has an unparseable color (fg=%q bg=%q)", name, p.label, p.fg, p.bg)
				continue
			}
			if ratio < minContrast {
				t.Errorf("theme %q: %s contrast %.2f < %.1f", name, p.label, ratio, minContrast)
			}
		}
	}
}

// TestOnColorFlips sanity-checks the helper: light background → dark text, dark
// background → light text.
func TestOnColorFlips(t *testing.T) {
	if got := OnColor(lipgloss.Color("#FFFFFF")); got != onDarkText {
		t.Errorf("OnColor(white) = %q, want dark text %q", got, onDarkText)
	}
	if got := OnColor(lipgloss.Color("#000000")); got != onLightText {
		t.Errorf("OnColor(black) = %q, want light text %q", got, onLightText)
	}
}
