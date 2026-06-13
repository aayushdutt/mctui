package ui

import (
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/progress"
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
	s.SelectedTitle = s.SelectedTitle.Foreground(accent).BorderLeftForeground(accent)
	s.SelectedDesc = s.SelectedDesc.Foreground(accentSoft).BorderLeftForeground(accent)
	s.NormalTitle = s.NormalTitle.Foreground(Active.Text)
	s.NormalDesc = s.NormalDesc.Foreground(Active.TextMuted)
	s.DimmedTitle = s.DimmedTitle.Foreground(Active.TextMuted)
	s.DimmedDesc = s.DimmedDesc.Foreground(Active.TextFaint)
	s.FilterMatch = s.FilterMatch.Foreground(accent).Underline(true)
	return d
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

// ThemeProgress builds a progress bar whose fill gradient comes from the active
// theme instead of bubbles' default pink→purple.
func ThemeProgress(width int) progress.Model {
	return progress.New(
		progress.WithGradient(string(Active.Primary), string(Active.Secondary)),
		progress.WithWidth(width),
	)
}
