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
)

// Shared styles
var (
	// Container styles
	ContainerStyle = lipgloss.NewStyle().
			Padding(1, 2)

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
