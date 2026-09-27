package ui

import (
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/lipgloss"
)

// This file centralizes theming for the bubbles widgets (list, textinput,
// progress). Those components ship with their own default colors; without these
// helpers the unselected list rows, filter box, pagination dots, placeholder
// text, text cursor, and progress bar would ignore the active theme. Route every
// widget through here so a theme covers the whole UI — and so a future theme or
// widget gets consistent treatment in one place.

// ThemeListDelegate returns a default list delegate fully dressed from the
// active theme. accent/accentSoft color the selected row; every other state
// (normal, dimmed, filter-match) uses theme neutrals instead of bubbles'
// built-in defaults. Foregrounds are overlaid on the default styles so spacing
// and the selected-row left border are preserved.
func ThemeListDelegate(accent, accentSoft lipgloss.Color) list.DefaultDelegate {
	d := list.NewDefaultDelegate()
	s := &d.Styles
	// Selected row: the accent › pointer on the title line (kit SelectMark /
	// rowMarker) and a bright, bold title; the description stays quiet. The
	// pointer is drawn as a one-glyph left "border" on the title only — a bar
	// down both lines would break into dashes in terminals with line spacing.
	// accentSoft is kept in the signature for callers but no longer used.
	_ = accentSoft
	s.SelectedTitle = s.SelectedTitle.Border(lipgloss.Border{Left: SelectMark}, false, false, false, true).
		Foreground(Active.TextStrong).Bold(true).BorderLeftForeground(accent)
	s.SelectedDesc = s.SelectedDesc.Border(lipgloss.Border{Left: " "}, false, false, false, true).
		Foreground(Active.TextDim)
	s.NormalTitle = s.NormalTitle.Foreground(Active.Text)
	s.NormalDesc = s.NormalDesc.Foreground(Active.TextMuted)
	s.DimmedTitle = s.DimmedTitle.Foreground(Active.TextMuted)
	s.DimmedDesc = s.DimmedDesc.Foreground(Active.TextFaint)
	s.FilterMatch = s.FilterMatch.Foreground(accent).Underline(true)
	return d
}

// ThemedListConfig configures NewThemedList. Accent/AccentSoft color the
// selected row; the booleans toggle the per-list options that actually vary
// between screens. Everything else (no title, quit keys disabled, themed chrome)
// is a shared default.
type ThemedListConfig struct {
	Accent, AccentSoft lipgloss.Color
	// StatusBar shows the list's own "N items" / filter status line. Pane titles
	// already carry counts, so only a filterable list wants it (it's where an
	// active filter is shown).
	StatusBar bool
	// ItemName / ItemNamePlural name the rows ("mod"/"mods"), used by the
	// status bar and the empty state ("No mods."). Default "item"/"items".
	ItemName, ItemNamePlural string
	// Filter enables the "/" filter.
	Filter bool
	// SingleLine renders compact one-line rows (no description, no row spacing) —
	// used for the category tree.
	SingleLine bool
	// Tight keeps the description line but drops the blank line between rows.
	Tight bool
}

// NewThemedList builds a bubbles list dressed from the active theme with the
// shared screen defaults: no title, no built-in help line (every screen has one
// footer; the list's own "↑/k up • ↓/j down • ? more" duplicated it, and its "?"
// never worked), quit keys disabled, themed chrome.
func NewThemedList(cfg ThemedListConfig) list.Model {
	l, _ := newFocusableList(cfg, nil)
	return l
}

// newFocusableList is [NewThemedList] for one pane of a multi-pane screen:
// focused reports whether the pane has focus, and the list hides its selected
// row while it doesn't. It also returns the delegate (for per-screen tweaks).
func newFocusableList(cfg ThemedListConfig, focused func() bool) (list.Model, *styledTitleDelegate) {
	del := ThemeListDelegate(cfg.Accent, cfg.AccentSoft)
	if cfg.SingleLine {
		del.ShowDescription = false
		del.SetSpacing(0)
	}
	if cfg.Tight {
		del.SetSpacing(0)
	}
	sd := &styledTitleDelegate{DefaultDelegate: del, focused: focused}
	l := list.New([]list.Item{}, sd, 0, 0)
	l.SetShowTitle(false)
	l.SetShowHelp(false)
	l.SetShowStatusBar(cfg.StatusBar)
	if cfg.ItemName != "" {
		l.SetStatusBarItemName(cfg.ItemName, cfg.ItemNamePlural)
	}
	l.SetFilteringEnabled(cfg.Filter)
	l.DisableQuitKeybindings()
	// Page with PgUp/PgDn only: the defaults (h/l/←/→/f/b/u/d) collide with
	// screen keys — ←/→ switch panes, and letters are commands.
	l.KeyMap.PrevPage.SetKeys("pgup")
	l.KeyMap.NextPage.SetKeys("pgdown")
	ThemeListChrome(&l)
	return l, sd
}

// ThemeListChrome styles a list's surrounding chrome — the filter input,
// pagination dots, status bar, and empty state — from the active theme. Call
// after list.New and any SetShow* toggles.
func ThemeListChrome(l *list.Model) {
	s := &l.Styles
	s.FilterPrompt = s.FilterPrompt.Foreground(Active.Primary)
	s.FilterCursor = s.FilterCursor.Foreground(Active.Primary)
	s.DefaultFilterCharacterMatch = s.DefaultFilterCharacterMatch.Foreground(Active.Primary).Underline(true)
	s.StatusBar = s.StatusBar.Foreground(Active.TextSubtle)
	s.StatusEmpty = s.StatusEmpty.Foreground(Active.TextMuted)
	s.StatusBarActiveFilter = s.StatusBarActiveFilter.Foreground(Active.Text)
	s.StatusBarFilterCount = s.StatusBarFilterCount.Foreground(Active.TextMuted)
	s.NoItems = s.NoItems.Foreground(Active.TextMuted)
	s.ActivePaginationDot = s.ActivePaginationDot.Foreground(Active.Primary)
	s.InactivePaginationDot = s.InactivePaginationDot.Foreground(Active.BorderSubtle)
	s.DividerDot = s.DividerDot.Foreground(Active.BorderSubtle)
}

// ThemeTextInput styles a text input — prompt, typed text, placeholder, and the
// blinking cursor — from the active theme.
func ThemeTextInput(ti *textinput.Model) {
	ti.PromptStyle = lipgloss.NewStyle().Foreground(Active.TextDim)
	ti.TextStyle = lipgloss.NewStyle().Foreground(Active.Text)
	ti.PlaceholderStyle = lipgloss.NewStyle().Foreground(Active.TextMuted)
	ti.Cursor.Style = lipgloss.NewStyle().Foreground(Active.Primary)
}
