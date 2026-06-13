// Package ui theming. A Palette is the full set of semantic color roles the UI
// renders against. Every view reads colors through the active palette
// ([Active]) — never hardcode a color in a view. Swapping the palette
// re-themes the whole app on the next render, because views build their
// lipgloss styles inline each frame.
package ui

import "github.com/charmbracelet/lipgloss"

// Palette maps semantic roles (what a color is *for*) to concrete colors. Add a
// theme by registering a filled-in Palette in themes.go; the compiler and the
// theme tests ensure every role is set.
type Palette struct {
	// Name is the registry key and the value persisted in config ("dark").
	Name string

	// Brand / accent
	Primary      lipgloss.Color // dominant brand accent: titles, focus, bars
	PrimaryDeep  lipgloss.Color // selected-row background
	Secondary    lipgloss.Color // secondary accent: browse pane, links
	AccentSoft   lipgloss.Color // light accent: section titles
	AccentSofter lipgloss.Color // palest accent: brand wordmark

	// Background is the app-shell background. Empty means "inherit the terminal
	// background", which every built-in theme uses today — themes recolor
	// foregrounds and let the terminal own the backdrop. Reserved for a future
	// shell-compositing pass.
	Background lipgloss.Color

	// Text hierarchy, brightest to faintest
	TextStrong lipgloss.Color // brightest: primary item titles
	Text       lipgloss.Color // default foreground
	Title      lipgloss.Color // section / screen titles
	TextSubtle lipgloss.Color // secondary copy
	TextDim    lipgloss.Color // labels, placeholders
	TextMuted  lipgloss.Color // dim labels, hints
	TextFaint  lipgloss.Color // help text, log output

	// Structure
	Border       lipgloss.Color // default panel border
	BorderSubtle lipgloss.Color // dividers, inactive borders
	BorderFaint  lipgloss.Color // faint borders / faint backgrounds

	// Status — success (emerald family)
	Success       lipgloss.Color // selection & "done"
	SuccessAccent lipgloss.Color // brighter success accent
	SuccessSoft   lipgloss.Color // light success
	SuccessFaint  lipgloss.Color // badge text
	SuccessBg     lipgloss.Color // badge background

	// Status — warning (amber family)
	Warning       lipgloss.Color
	WarningStrong lipgloss.Color // emphasized warning ("running" step)
	WarningSoft   lipgloss.Color
	WarningBg     lipgloss.Color // badge background

	// Status — error
	Error lipgloss.Color
}

// AutoTheme adapts to the terminal: applying it resolves to [darkTheme] or
// [lightTheme] based on the detected terminal background, so the default
// experience stays readable on both light and dark terminals without painting a
// background. It is not a stored palette — it is resolved fresh each [Apply].
const AutoTheme = "auto"

// Active is the palette every view renders against. Swapped by [Apply]; seeded
// to the dark theme in themes.go's init.
var Active Palette

var (
	themeRegistry = map[string]Palette{}
	themeOrder    []string // registration order, for stable picker/listing
)

// registerTheme adds p to the registry. Called from themes.go init.
func registerTheme(p Palette) {
	if _, exists := themeRegistry[p.Name]; !exists {
		themeOrder = append(themeOrder, p.Name)
	}
	themeRegistry[p.Name] = p
}

// Apply switches the active palette to the named theme. It reports whether the
// theme exists; on an unknown name Active is left unchanged. [AutoTheme]
// resolves to the dark or light palette based on the terminal background.
func Apply(name string) bool {
	if name == AutoTheme {
		Active = resolveAuto()
		return true
	}
	p, ok := themeRegistry[name]
	if ok {
		Active = p
	}
	return ok
}

// resolveAuto picks the dark or light palette to match the terminal, keeping the
// "auto" name so the choice round-trips through config and the picker.
func resolveAuto() Palette {
	p := lightTheme
	if lipgloss.HasDarkBackground() {
		p = darkTheme
	}
	p.Name = AutoTheme
	return p
}

// HasTheme reports whether name is a selectable theme (a registered palette or
// [AutoTheme]).
func HasTheme(name string) bool {
	if name == AutoTheme {
		return true
	}
	_, ok := themeRegistry[name]
	return ok
}

// ActiveName returns the name of the active theme.
func ActiveName() string { return Active.Name }

// ThemeNames returns the selectable theme names: AutoTheme first, then the
// registered palettes in registration order.
func ThemeNames() []string {
	out := make([]string, 0, len(themeOrder)+1)
	out = append(out, AutoTheme)
	return append(out, themeOrder...)
}
