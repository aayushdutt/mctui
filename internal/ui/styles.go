// Package ui shared layout primitives. Colors live in the active theme
// ([Active] in theme.go); views read roles like Active.Primary directly and
// build their lipgloss styles inline so a theme swap repaints everything.
package ui

import "github.com/charmbracelet/lipgloss"

// App shell: consistent inset from the terminal edge for every full-screen view.
// Apply in the root View after sizing children to (terminal − 2*pad).
const (
	AppShellPadY = 1
	AppShellPadX = 2
)

// AppShellStyle wraps rendered views with the standard horizontal/vertical
// padding. The root View applies the active theme's Background on top.
var AppShellStyle = lipgloss.NewStyle().Padding(AppShellPadY, AppShellPadX)
