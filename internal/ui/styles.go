// Package ui styles contains shared styling definitions.
// Centralized styles ensure visual consistency across all views.
package ui

import "github.com/charmbracelet/lipgloss"

// Color palette - using a cohesive purple/violet theme
var (
	ColorPrimary   = lipgloss.Color("#7C3AED") // Violet
	ColorSecondary = lipgloss.Color("#A78BFA") // Light violet
	ColorAccent    = lipgloss.Color("#34D399") // Emerald (success)
	ColorWarning   = lipgloss.Color("#FBBF24") // Amber
	ColorError     = lipgloss.Color("#EF4444") // Red
	ColorMuted     = lipgloss.Color("#626262") // Gray
	ColorText      = lipgloss.Color("#FAFAFA") // White
	ColorSubtle    = lipgloss.Color("#A1A1AA") // Zinc

	// Violet / purple accents
	ColorPrimaryDeep = lipgloss.Color("#6D28D9") // Deep violet (selected row background)
	ColorViolet300   = lipgloss.Color("#C4B5FD") // Light violet (Discover title)
	ColorViolet200   = lipgloss.Color("#E9D5FF") // Pale violet (Mods brand)

	// Emerald / green (success states)
	ColorSuccess       = lipgloss.Color("#10B981") // Emerald (selection & "done")
	ColorSuccessSubtle = lipgloss.Color("#6EE7B7") // Light emerald
	ColorSuccessFaint  = lipgloss.Color("#A7F3D0") // Pale emerald (version badge text)
	ColorSuccessBg     = lipgloss.Color("#14532D") // Dark emerald (version badge background)

	// Amber / yellow
	ColorAmber       = lipgloss.Color("#F59E0B") // Amber ("running" step)
	ColorAmberSubtle = lipgloss.Color("#FCD34D") // Light amber
	ColorAmberBg     = lipgloss.Color("#422006") // Dark amber (loader badge background)

	// Zinc neutrals (text, labels, borders, backgrounds)
	ColorZinc200 = lipgloss.Color("#E4E4E7") // Light zinc (titles)
	ColorZinc100 = lipgloss.Color("#F4F4F5") // Lightest zinc (mod title)
	ColorZinc500 = lipgloss.Color("#71717A") // Zinc (placeholder/label text)
	ColorZinc600 = lipgloss.Color("#52525B") // Zinc (dim labels)
	ColorZinc700 = lipgloss.Color("#3F3F46") // Zinc (subtle borders/dividers)
	ColorZinc800 = lipgloss.Color("#27272A") // Zinc (faint borders/backgrounds)

	// Stone neutrals (installed-pane accents)
	ColorStone    = lipgloss.Color("#57534E") // Stone (installed-pane border)
	ColorStone500 = lipgloss.Color("#78716C") // Stone (panel border)
	ColorStone400 = lipgloss.Color("#A8A29E") // Light stone (actions text)

	// Misc grays
	ColorGray = lipgloss.Color("#555555") // Gray (log text)
)

// App shell: consistent inset from the terminal edge for every full-screen view.
// Apply in the root View after sizing children to (terminal − 2*pad).
const (
	AppShellPadY = 1
	AppShellPadX = 2
)

// AppShellStyle wraps rendered views with the standard horizontal/vertical padding.
var AppShellStyle = lipgloss.NewStyle().Padding(AppShellPadY, AppShellPadX)

// Shared styles
var (
	// ContainerStyle matches [AppShellStyle] (legacy alias).
	ContainerStyle = AppShellStyle

	// Title styles
	TitleStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(ColorText).
			Background(ColorPrimary).
			Padding(0, 1)

	// Help text style
	HelpStyle = lipgloss.NewStyle().
			Foreground(ColorMuted)

	// Selected item style
	SelectedStyle = lipgloss.NewStyle().
			Foreground(ColorPrimary).
			Bold(true)

	// Error message style
	ErrorStyle = lipgloss.NewStyle().
			Foreground(ColorError).
			Bold(true)

	// Success message style
	SuccessStyle = lipgloss.NewStyle().
			Foreground(ColorAccent).
			Bold(true)

	// Box styles for panels
	BoxStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(ColorMuted).
			Padding(1, 2)

	FocusedBoxStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(ColorPrimary).
			Padding(1, 2)
)
