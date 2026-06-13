// Package ui contains all TUI view components.
// Each view is a Bubbletea model that can be composed into the main app.
package ui

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/loader"
	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// HomeModel is the main instance list view
type HomeModel struct {
	list      list.Model
	instances []*core.Instance
	width     int
	height    int
	keys      homeKeyMap
	loading   bool
	accounts  *core.AccountManager

	// Microsoft session hint from background check (MSA accounts only)
	sessionRemote sessionRemoteLine

	// transientBanner is a one-line notice (e.g. session gate network error)
	transientBanner string

	// Delete confirmation state
	confirmDelete  bool
	deleteTarget   *core.Instance
	deleteFocusYes bool // which option arrows / Enter apply to (default Yes so Enter still confirms delete)
}

type sessionRemoteLine int

const (
	sessionRemoteUnset sessionRemoteLine = iota
	sessionRemoteChecking
	sessionRemoteInvalid
	sessionRemoteUncertain
)

type homeKeyMap struct {
	Launch      key.Binding
	PlayOffline key.Binding
	NewInst     key.Binding
	Mods        key.Binding
	Settings    key.Binding
	Delete      key.Binding
	Auth        key.Binding
	OpenFolder  key.Binding
}

func defaultHomeKeyMap() homeKeyMap {
	return homeKeyMap{
		Launch: key.NewBinding(
			key.WithKeys("enter", "l"),
			key.WithHelp("enter", "launch"),
		),
		PlayOffline: key.NewBinding(
			key.WithKeys("o"),
			key.WithHelp("o", "play offline"),
		),
		NewInst: key.NewBinding(
			key.WithKeys("n"),
			key.WithHelp("n", "new"),
		),
		Mods: key.NewBinding(
			key.WithKeys("m"),
			key.WithHelp("m", "mods"),
		),
		Settings: key.NewBinding(
			key.WithKeys("s"),
			key.WithHelp("s", "settings"),
		),
		Delete: key.NewBinding(
			key.WithKeys("d"),
			key.WithHelp("d", "delete"),
		),
		Auth: key.NewBinding(
			key.WithKeys("a"),
			key.WithHelp("a", "accounts"),
		),
		OpenFolder: key.NewBinding(
			key.WithKeys("f"),
			key.WithHelp("f", "open folder"),
		),
	}
}

// instanceItem represents a Minecraft instance in the list
type instanceItem struct {
	instance *core.Instance
}

func (i instanceItem) Title() string { return i.instance.Name }

func (i instanceItem) Description() string {
	loader := i.instance.Loader
	if loader == "" || loader == "vanilla" {
		loader = "Vanilla"
	}

	lastPlayed := "Never played"
	if !i.instance.LastPlayed.IsZero() {
		lastPlayed = formatRelativeTime(i.instance.LastPlayed)
	}

	return fmt.Sprintf("%s • %s • %s", i.instance.Version, loader, lastPlayed)
}
func (i instanceItem) FilterValue() string { return i.instance.Name }

func formatRelativeTime(t time.Time) string {
	diff := time.Since(t)
	switch {
	case diff < time.Minute:
		return "Just now"
	case diff < time.Hour:
		return fmt.Sprintf("%dm ago", int(diff.Minutes()))
	case diff < 24*time.Hour:
		return fmt.Sprintf("%dh ago", int(diff.Hours()))
	case diff < 7*24*time.Hour:
		return fmt.Sprintf("%dd ago", int(diff.Hours()/24))
	default:
		return t.Format("Jan 2")
	}
}

// NewHomeModel creates a new home view model
func NewHomeModel() *HomeModel {
	base := ThemeListDelegate(Active.Primary, Active.Secondary)

	l := list.New([]list.Item{}, &homeInstanceDelegate{DefaultDelegate: base}, 0, 0)
	l.Title = "🎮 Minecraft Instances"
	l.SetShowStatusBar(true)
	l.SetFilteringEnabled(true)
	l.SetShowTitle(true)
	l.Styles.Title = homeTitleStyle()
	l.SetShowHelp(false)
	ThemeListChrome(&l)

	return &HomeModel{
		list:    l,
		keys:    defaultHomeKeyMap(),
		loading: true,
	}
}

// homeTitleStyle is the list's title bar, themed from the active palette.
func homeTitleStyle() lipgloss.Style {
	return lipgloss.NewStyle().
		Bold(true).
		Foreground(OnColor(Active.Primary)).
		Background(Active.Primary).
		Padding(0, 1)
}

// ApplyTheme re-dresses the (long-lived) home list from the active theme. The
// app calls this after the theme changes so the home screen — which, unlike the
// per-entry screens, is not rebuilt on navigation — picks up the new palette.
func (m *HomeModel) ApplyTheme() {
	base := ThemeListDelegate(Active.Primary, Active.Secondary)
	m.list.SetDelegate(&homeInstanceDelegate{DefaultDelegate: base})
	m.list.Styles.Title = homeTitleStyle()
	ThemeListChrome(&m.list)
}

// SetInstances updates the instance list. If selectID is non-empty, the cursor moves to that instance (usually index 0 for a new instance).
func (m *HomeModel) SetInstances(instances []*core.Instance, selectID string) {
	m.instances = instances
	m.loading = false

	// Sort by max(LastPlayed, CreatedAt) — newest first
	sort.Slice(instances, func(i, j int) bool {
		return core.RecencyForSort(instances[i]).After(core.RecencyForSort(instances[j]))
	})

	items := make([]list.Item, len(instances))
	for i, inst := range instances {
		items[i] = instanceItem{instance: inst}
	}
	m.list.SetItems(items)

	if selectID != "" {
		for i, inst := range instances {
			if inst.ID == selectID {
				m.list.Select(i)
				return
			}
		}
		m.list.Select(0)
	}
}

func (m *HomeModel) SetAccountManager(am *core.AccountManager) {
	m.accounts = am
}

// SetSessionCheckStarted marks the UI as verifying the active Microsoft session.
func (m *HomeModel) SetSessionCheckStarted() {
	if acc := m.activeMSAAccount(); acc != nil {
		m.sessionRemote = sessionRemoteChecking
	}
}

// ApplyActiveSessionCheckResult updates session status from a background check.
func (m *HomeModel) ApplyActiveSessionCheckResult(res ActiveSessionCheckResult) {
	switch res.Status {
	case ActiveSessionNotApplicable:
		m.sessionRemote = sessionRemoteUnset
	case ActiveSessionOK:
		m.sessionRemote = sessionRemoteUnset
	case ActiveSessionInvalid:
		m.sessionRemote = sessionRemoteInvalid
	case ActiveSessionUncertain:
		m.sessionRemote = sessionRemoteUncertain
	}
}

// SetTransientBanner shows a short notice above the footer; cleared on the next key press.
func (m *HomeModel) SetTransientBanner(s string) {
	m.transientBanner = s
	m.applyListSize()
}

func (m *HomeModel) activeMSAAccount() *core.Account {
	if m.accounts == nil {
		return nil
	}
	acc := m.accounts.GetActive()
	if acc == nil || acc.Type != core.AccountTypeMSA {
		return nil
	}
	return acc
}

func (m *HomeModel) applyListSize() {
	footerLines := 6
	if m.transientBanner != "" {
		footerLines++
	}
	m.list.SetSize(m.width, m.height-footerLines)
}

// SelectedInstance returns the currently selected instance
func (m *HomeModel) SelectedInstance() *core.Instance {
	if item, ok := m.list.SelectedItem().(instanceItem); ok {
		return item.instance
	}
	return nil
}

// instanceSupportsModsBrowser is true when the in-app Mods (Modrinth) screen applies.
// Expand this when additional loaders are wired up the same way.
func instanceSupportsModsBrowser(inst *core.Instance) bool {
	if inst == nil {
		return false
	}
	return loader.ParseKind(inst.Loader) == loader.KindFabric
}

// SetSize updates the dimensions of the home view
func (m *HomeModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.applyListSize()
}

// Init implements tea.Model
func (m *HomeModel) Init() tea.Cmd {
	return nil
}

// Update implements tea.Model
func (m *HomeModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case InstancesLoaded:
		if msg.Error == nil {
			m.SetInstances(msg.Instances, msg.SelectID)
		}
		return m, nil

	case tea.KeyMsg:
		if m.transientBanner != "" {
			m.transientBanner = ""
			m.applyListSize()
		}

		// Handle delete confirmation mode
		if m.confirmDelete {
			switch msg.String() {
			case "up", "left":
				m.deleteFocusYes = true
				return m, nil
			case "down", "right":
				m.deleteFocusYes = false
				return m, nil
			case "k":
				m.deleteFocusYes = true
				return m, nil
			case "j":
				m.deleteFocusYes = false
				return m, nil
			case "tab":
				m.deleteFocusYes = !m.deleteFocusYes
				return m, nil
			case "y", "Y":
				inst := m.deleteTarget
				m.confirmDelete = false
				m.deleteTarget = nil
				return m, func() tea.Msg { return DeleteInstance{Instance: inst} }
			case "enter":
				if m.deleteFocusYes {
					inst := m.deleteTarget
					m.confirmDelete = false
					m.deleteTarget = nil
					return m, func() tea.Msg { return DeleteInstance{Instance: inst} }
				}
				m.confirmDelete = false
				m.deleteTarget = nil
				return m, nil
			case "n", "N", "esc", "q":
				m.confirmDelete = false
				m.deleteTarget = nil
				return m, nil
			}
			return m, nil
		}

		// Don't handle keys if filtering
		if m.list.FilterState() == list.Filtering {
			break
		}

		switch {
		case key.Matches(msg, m.keys.Launch):
			if inst := m.SelectedInstance(); inst != nil {
				// If authenticated, launch online. Else go to auth.
				if m.accounts != nil && m.accounts.GetActive() != nil {
					return m, func() tea.Msg { return NavigateToLaunch{Instance: inst, Offline: false} }
				}
				// Not authenticated - go to login
				return m, func() tea.Msg { return NavigateToAuth{} }
			}
		case key.Matches(msg, m.keys.PlayOffline):
			if inst := m.SelectedInstance(); inst != nil {
				return m, func() tea.Msg { return NavigateToLaunch{Instance: inst, Offline: true} }
			}
		case key.Matches(msg, m.keys.NewInst):
			return m, func() tea.Msg { return NavigateToNewInstance{} }
		case key.Matches(msg, m.keys.Mods):
			if inst := m.SelectedInstance(); inst != nil {
				if !instanceSupportsModsBrowser(inst) {
					m.SetTransientBanner("Mods browser isn't available for this instance.")
					return m, nil
				}
				return m, func() tea.Msg { return NavigateToMods{Instance: inst} }
			}
		case key.Matches(msg, m.keys.Settings):
			return m, func() tea.Msg { return NavigateToSettings{} }
		case key.Matches(msg, m.keys.Auth):
			return m, func() tea.Msg { return NavigateToAuth{} }
		case key.Matches(msg, m.keys.OpenFolder):
			if inst := m.SelectedInstance(); inst != nil {
				openInstanceFolder(inst.Path)
			}
		case key.Matches(msg, m.keys.Delete):
			if inst := m.SelectedInstance(); inst != nil {
				// Show confirmation prompt (Yes focused: Enter deletes; arrows or Tab change choice)
				m.confirmDelete = true
				m.deleteTarget = inst
				m.deleteFocusYes = true
				return m, nil
			}
		}
	}

	var cmd tea.Cmd
	m.list, cmd = m.list.Update(msg)
	return m, cmd
}

// View implements tea.Model
func (m *HomeModel) View() string {
	if m.loading {
		return lipgloss.NewStyle().
			Foreground(Active.TextSubtle).
			Render("Loading instances...")
	}

	if len(m.instances) == 0 {
		// Delightful empty state with project intro
		title := lipgloss.NewStyle().
			Bold(true).
			Foreground(Active.Primary).
			Render(BrandGlyph + "  mctui")

		tagline := lipgloss.NewStyle().
			Foreground(Active.TextSubtle).
			Italic(true).
			Render("A terminal-based Minecraft launcher")

		// Size the rule to the content (cap it so it stays a tidy underline).
		ruleW := m.width / 3
		if ruleW > 40 {
			ruleW = 40
		}
		if ruleW < 12 {
			ruleW = 12
		}
		divider := Rule(ruleW)

		emptyMsg := lipgloss.NewStyle().
			Foreground(Active.Text).
			MarginTop(1).
			Render("No instances yet. Let's get started!")

		tips := lipgloss.NewStyle().
			MarginTop(1).
			Render(KeyHints(
				ruleW,
				KeyHint{"n", "new instance"},
				KeyHint{"a", "add account"},
				KeyHint{"s", "settings"},
				KeyHint{"q", "quit"},
			))

		content := lipgloss.JoinVertical(
			lipgloss.Center,
			title,
			tagline,
			"",
			divider,
			"",
			emptyMsg,
			tips,
		)

		// Center the content
		return lipgloss.Place(
			m.width,
			m.height,
			lipgloss.Center,
			lipgloss.Center,
			content,
		)
	}

	// Auth line: avoid "signed in" when Microsoft session is invalid or unverified.
	authStatus := "Not signed in"
	authWarn := false
	if m.accounts != nil {
		if acc := m.accounts.GetActive(); acc != nil {
			switch acc.Type {
			case core.AccountTypeMSA:
				switch m.sessionRemote {
				case sessionRemoteInvalid:
					authStatus = fmt.Sprintf("%s — Microsoft sign-in required [a]", acc.Name)
					authWarn = true
				case sessionRemoteUncertain:
					authStatus = fmt.Sprintf("%s — can't verify session (network?) [a]", acc.Name)
					authWarn = true
				case sessionRemoteChecking:
					authStatus = fmt.Sprintf("Checking Microsoft session for %s…", acc.Name)
				default:
					authStatus = fmt.Sprintf("Signed in as %s", acc.Name)
				}
			case core.AccountTypeOffline:
				authStatus = fmt.Sprintf("Offline: %s", acc.Name)
			default:
				authStatus = fmt.Sprintf("Account: %s", acc.Name)
			}
		}
	}

	msaNeedsSignin := false
	if acc := m.activeMSAAccount(); acc != nil {
		switch m.sessionRemote {
		case sessionRemoteInvalid, sessionRemoteUncertain:
			msaNeedsSignin = true
		}
	}

	// Build help items based on auth status.
	noOnlineSession := m.accounts == nil || m.accounts.GetActive() == nil || msaNeedsSignin
	launchLabel := "launch"
	if noOnlineSession {
		launchLabel = "login & play"
	}
	modsLabel := "mods"
	if inst := m.SelectedInstance(); inst != nil && !instanceSupportsModsBrowser(inst) {
		modsLabel = "mods (unsupported)"
	}
	helpItems := []KeyHint{
		{"↵", launchLabel},
		{"n", "new"},
		{"f", "folder"},
		{"d", "delete"},
		{"m", modsLabel},
		{"s", "settings"},
		{"a", "accounts"},
		{"o", "play offline"},
		{"q", "quit"},
	}

	help := KeyHints(m.width, helpItems...)

	statusColor := Active.Primary
	statusGlyph := GlyphDot
	if authWarn {
		statusColor = Active.Warning
		statusGlyph = GlyphWarn
	}
	status := lipgloss.NewStyle().
		Foreground(statusColor).
		Padding(1, 0).
		Render(statusGlyph + " " + authStatus)

	aboveStatus := []string{m.list.View()}
	if m.transientBanner != "" {
		aboveStatus = append(aboveStatus, lipgloss.NewStyle().
			Foreground(Active.Warning).
			Render(GlyphWarn+" "+m.transientBanner))
	}
	aboveStatus = append(aboveStatus, status, help)

	baseView := lipgloss.JoinVertical(lipgloss.Left, aboveStatus...)

	// Show delete confirmation overlay if needed
	if m.confirmDelete && m.deleteTarget != nil {
		panelW := m.width - 8
		if panelW > 56 {
			panelW = 56
		}
		if panelW < 24 {
			panelW = 24
		}

		warn := lipgloss.NewStyle().
			Bold(true).
			Foreground(Active.Error).
			Render(GlyphWarn + " This cannot be undone.")

		msg := lipgloss.NewStyle().
			Foreground(Active.TextSubtle).
			Render(fmt.Sprintf("Delete \"%s\"?", m.deleteTarget.Name))

		yesLbl := "Yes, delete"
		noLbl := "No, cancel"
		yesSt := lipgloss.NewStyle().Foreground(Active.TextFaint)
		noSt := lipgloss.NewStyle().Foreground(Active.TextFaint)
		if m.deleteFocusYes {
			yesSt = lipgloss.NewStyle().Bold(true).Foreground(Active.Error)
		} else {
			noSt = lipgloss.NewStyle().Bold(true).Foreground(Active.Text)
		}
		options := lipgloss.NewStyle().
			MarginTop(1).
			Render(lipgloss.JoinHorizontal(lipgloss.Left,
				yesSt.Render(yesLbl),
				lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render("  ·  "),
				noSt.Render(noLbl),
			))

		body := lipgloss.JoinVertical(
			lipgloss.Left,
			msg,
			"",
			warn,
			options,
		)
		panel := Panel("Delete instance?", body, panelW, Active.Error)

		hint := lipgloss.NewStyle().
			MarginTop(1).
			Render(KeyHints(
				panelW,
				KeyHint{"↑↓/Tab", "choose"},
				KeyHint{"↵", "confirm"},
				KeyHint{"y/n", ""},
				KeyHint{"esc", "cancel"},
			))

		promptBox := lipgloss.JoinVertical(lipgloss.Left, panel, hint)

		return lipgloss.Place(
			m.width,
			m.height,
			lipgloss.Center,
			lipgloss.Center,
			promptBox,
		)
	}

	return baseView
}

// openInstanceFolder opens the instance folder in the system file manager
func openInstanceFolder(path string) {
	mcDir := filepath.Join(path, ".minecraft")
	_ = os.MkdirAll(mcDir, 0755)
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", mcDir)
	case "linux":
		cmd = exec.Command("xdg-open", mcDir)
	case "windows":
		cmd = exec.Command("explorer", mcDir)
	default:
		return
	}
	_ = cmd.Start()
}

// buildHelpText builds help text with item-wise wrapping.
// Items are kept together; lines wrap at item boundaries. Production help (Home
// and Mods) now uses the shared KeyHints kit; this remains only for its own unit
// tests and can be retired with them.
func buildHelpText(items []string, maxWidth int) string {
	if maxWidth <= 0 {
		maxWidth = 80
	}

	separator := " • "
	var lines []string
	var currentLine string

	for i, item := range items {
		testLine := currentLine
		if testLine != "" {
			testLine += separator
		}
		testLine += item

		// Check if adding this item would exceed width
		if len(testLine) > maxWidth && currentLine != "" {
			lines = append(lines, currentLine)
			currentLine = item
		} else {
			if currentLine != "" {
				currentLine += separator
			}
			currentLine += item
		}

		// Last item
		if i == len(items)-1 && currentLine != "" {
			lines = append(lines, currentLine)
		}
	}

	return strings.Join(lines, "\n")
}
