package ui

import "github.com/charmbracelet/lipgloss"

// Built-in themes. To add one: write a Palette filling every role and register
// it in init(). Names here are the values users put in config's "theme" key.
//
// Note: built-in themes recolor foregrounds, accents, borders, and badge
// backgrounds; they leave Background empty and inherit the terminal's backdrop.
// The "light" preset is tuned for light-background terminals.

func init() {
	registerTheme(darkTheme)
	registerTheme(lightTheme)
	registerTheme(gruvboxTheme)
	registerTheme(catppuccinTheme)
	registerTheme(draculaTheme)
	registerTheme(nordTheme)

	// Compile-time default; app.New overrides from config at startup.
	Active = themeRegistry["dark"]
}

func c(hex string) lipgloss.Color { return lipgloss.Color(hex) }

// darkTheme is the original palette — the default. Its hexes match the
// pre-theming UI exactly so existing users see no change.
var darkTheme = Palette{
	Name: "dark",

	Primary:      c("#7C3AED"),
	PrimaryDeep:  c("#6D28D9"),
	Secondary:    c("#A78BFA"),
	AccentSoft:   c("#C4B5FD"),
	AccentSofter: c("#E9D5FF"),

	Background: c(""),

	TextStrong: c("#F4F4F5"),
	Text:       c("#FAFAFA"),
	Title:      c("#E4E4E7"),
	TextSubtle: c("#A1A1AA"),
	TextDim:    c("#71717A"),
	TextMuted:  c("#52525B"),
	TextFaint:  c("#626262"),

	Border:       c("#78716C"),
	BorderSubtle: c("#3F3F46"),
	BorderFaint:  c("#27272A"),

	Success:       c("#10B981"),
	SuccessAccent: c("#34D399"),
	SuccessSoft:   c("#6EE7B7"),
	SuccessFaint:  c("#A7F3D0"),
	SuccessBg:     c("#14532D"),

	Warning:       c("#FBBF24"),
	WarningStrong: c("#F59E0B"),
	WarningSoft:   c("#FCD34D"),
	WarningBg:     c("#422006"),

	Error: c("#EF4444"),
}

// lightTheme is tuned for light-background terminals: dark foregrounds, light
// borders, and pale badge backgrounds with dark badge text.
var lightTheme = Palette{
	Name: "light",

	Primary:      c("#6D28D9"),
	PrimaryDeep:  c("#DDD6FE"), // pale violet: dark row text reads on top
	Secondary:    c("#7C3AED"),
	AccentSoft:   c("#6D28D9"),
	AccentSofter: c("#5B21B6"),

	Background: c(""),

	TextStrong: c("#18181B"),
	Text:       c("#27272A"),
	Title:      c("#3F3F46"),
	TextSubtle: c("#52525B"),
	TextDim:    c("#71717A"),
	TextMuted:  c("#A1A1AA"),
	TextFaint:  c("#A1A1AA"),

	Border:       c("#D4D4D8"),
	BorderSubtle: c("#E4E4E7"),
	BorderFaint:  c("#F4F4F5"),

	Success:       c("#047857"),
	SuccessAccent: c("#059669"),
	SuccessSoft:   c("#065F46"),
	SuccessFaint:  c("#065F46"), // dark green text over the pale badge
	SuccessBg:     c("#A7F3D0"),

	Warning:       c("#B45309"),
	WarningStrong: c("#92400E"),
	WarningSoft:   c("#78350F"), // darker, for AA contrast as badge text on WarningBg
	WarningBg:     c("#FDE68A"),

	Error: c("#DC2626"),
}

// gruvboxTheme — retro warm palette (Gruvbox dark).
var gruvboxTheme = Palette{
	Name: "gruvbox",

	Primary:      c("#d3869b"),
	PrimaryDeep:  c("#b16286"),
	Secondary:    c("#83a598"),
	AccentSoft:   c("#d3869b"),
	AccentSofter: c("#ebdbb2"),

	Background: c(""),

	TextStrong: c("#fbf1c7"),
	Text:       c("#ebdbb2"),
	Title:      c("#ebdbb2"),
	TextSubtle: c("#d5c4a1"),
	TextDim:    c("#bdae93"),
	TextMuted:  c("#a89984"),
	TextFaint:  c("#928374"),

	Border:       c("#665c54"),
	BorderSubtle: c("#504945"),
	BorderFaint:  c("#3c3836"),

	Success:       c("#98971a"),
	SuccessAccent: c("#b8bb26"),
	SuccessSoft:   c("#b8bb26"),
	SuccessFaint:  c("#d8e9a8"),
	SuccessBg:     c("#32361a"),

	Warning:       c("#d79921"),
	WarningStrong: c("#fabd2f"),
	WarningSoft:   c("#fabd2f"),
	WarningBg:     c("#3c2f1a"),

	Error: c("#fb4934"),
}

// catppuccinTheme — soft pastel palette (Catppuccin Mocha).
var catppuccinTheme = Palette{
	Name: "catppuccin",

	Primary:      c("#cba6f7"),
	PrimaryDeep:  c("#6e5bb0"),
	Secondary:    c("#89b4fa"),
	AccentSoft:   c("#cba6f7"),
	AccentSofter: c("#b4befe"),

	Background: c(""),

	TextStrong: c("#cdd6f4"),
	Text:       c("#cdd6f4"),
	Title:      c("#bac2de"),
	TextSubtle: c("#a6adc8"),
	TextDim:    c("#7f849c"),
	TextMuted:  c("#6c7086"),
	TextFaint:  c("#585b70"),

	Border:       c("#585b70"),
	BorderSubtle: c("#45475a"),
	BorderFaint:  c("#313244"),

	Success:       c("#a6e3a1"),
	SuccessAccent: c("#a6e3a1"),
	SuccessSoft:   c("#94e2d5"),
	SuccessFaint:  c("#b9f5cf"),
	SuccessBg:     c("#2a3b2e"),

	Warning:       c("#f9e2af"),
	WarningStrong: c("#fab387"),
	WarningSoft:   c("#f9e2af"),
	WarningBg:     c("#3b2f1e"),

	Error: c("#f38ba8"),
}

// draculaTheme — high-contrast dark palette (Dracula).
var draculaTheme = Palette{
	Name: "dracula",

	Primary:      c("#bd93f9"),
	PrimaryDeep:  c("#6e5fae"),
	Secondary:    c("#ff79c6"),
	AccentSoft:   c("#bd93f9"),
	AccentSofter: c("#f8f8f2"),

	Background: c(""),

	TextStrong: c("#ffffff"),
	Text:       c("#f8f8f2"),
	Title:      c("#f8f8f2"),
	TextSubtle: c("#c8c8e0"),
	TextDim:    c("#8b8fb0"),
	TextMuted:  c("#6272a4"),
	TextFaint:  c("#6272a4"),

	Border:       c("#44475a"),
	BorderSubtle: c("#3a3d4d"),
	BorderFaint:  c("#2f3140"),

	Success:       c("#50fa7b"),
	SuccessAccent: c("#50fa7b"),
	SuccessSoft:   c("#69ff94"),
	SuccessFaint:  c("#b8ffcf"),
	SuccessBg:     c("#1d3b27"),

	Warning:       c("#f1fa8c"),
	WarningStrong: c("#ffb86c"),
	WarningSoft:   c("#f1fa8c"),
	WarningBg:     c("#3b3520"),

	Error: c("#ff5555"),
}

// nordTheme — cool arctic palette (Nord).
var nordTheme = Palette{
	Name: "nord",

	Primary:      c("#b48ead"),
	PrimaryDeep:  c("#5e81ac"),
	Secondary:    c("#88c0d0"),
	AccentSoft:   c("#b48ead"),
	AccentSofter: c("#d8dee9"),

	Background: c(""),

	TextStrong: c("#eceff4"),
	Text:       c("#e5e9f0"),
	Title:      c("#d8dee9"),
	TextSubtle: c("#c0c8d4"),
	TextDim:    c("#9aa3b2"),
	TextMuted:  c("#7b8494"),
	TextFaint:  c("#4c566a"),

	Border:       c("#4c566a"),
	BorderSubtle: c("#434c5e"),
	BorderFaint:  c("#3b4252"),

	Success:       c("#a3be8c"),
	SuccessAccent: c("#a3be8c"),
	SuccessSoft:   c("#b5cda0"),
	SuccessFaint:  c("#d2e0c4"),
	SuccessBg:     c("#313a2a"),

	Warning:       c("#ebcb8b"),
	WarningStrong: c("#d08770"),
	WarningSoft:   c("#ebcb8b"),
	WarningBg:     c("#3b3520"),

	Error: c("#bf616a"),
}
