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

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/quasar/mctui/internal/core"
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

	// Delete confirmation state
	confirmDelete bool
	deleteTarget  *core.Instance
}

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
	delegate := list.NewDefaultDelegate()
	delegate.Styles.SelectedTitle = delegate.Styles.SelectedTitle.
		Foreground(lipgloss.Color("#7C3AED")).
		BorderLeftForeground(lipgloss.Color("#7C3AED"))
	delegate.Styles.SelectedDesc = delegate.Styles.SelectedDesc.
		Foreground(lipgloss.Color("#A78BFA")).
		BorderLeftForeground(lipgloss.Color("#7C3AED"))

	l := list.New([]list.Item{}, delegate, 0, 0)
	l.Title = "🎮 Minecraft Instances"
	l.SetShowStatusBar(true)
	l.SetFilteringEnabled(true)
	l.SetShowTitle(true)
	l.Styles.Title = lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FAFAFA")).
		Background(lipgloss.Color("#7C3AED")).
		Padding(0, 1)
	l.SetShowHelp(false)

	return &HomeModel{
		list:    l,
		keys:    defaultHomeKeyMap(),
		loading: true,
	}
}

// SetInstances updates the instance list
func (m *HomeModel) SetInstances(instances []*core.Instance) {
	m.instances = instances
	m.loading = false

	// Sort by last played (most recent first)
	sort.Slice(instances, func(i, j int) bool {
		return instances[i].LastPlayed.After(instances[j].LastPlayed)
	})

	items := make([]list.Item, len(instances))
	for i, inst := range instances {
		items[i] = instanceItem{instance: inst}
	}
	m.list.SetItems(items)
}

func (m *HomeModel) SetAccountManager(am *core.AccountManager) {
	m.accounts = am
}

// SelectedInstance returns the currently selected instance
func (m *HomeModel) SelectedInstance() *core.Instance {
	if item, ok := m.list.SelectedItem().(instanceItem); ok {
		return item.instance
	}
	return nil
}

// SetSize updates the dimensions of the home view
func (m *HomeModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	// Reserve space for: status (3 lines with padding) + help text (up to 2 lines)
	m.list.SetSize(width, height-6)
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
			m.SetInstances(msg.Instances)
		}
		return m, nil

	case tea.KeyMsg:
		// Handle delete confirmation mode
		if m.confirmDelete {
			switch msg.String() {
			case "y", "Y", "enter":
				inst := m.deleteTarget
				m.confirmDelete = false
				m.deleteTarget = nil
				return m, func() tea.Msg { return DeleteInstance{Instance: inst} }
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
				// Show confirmation prompt
				m.confirmDelete = true
				m.deleteTarget = inst
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
			Foreground(lipgloss.Color("#A1A1AA")).
			Render("Loading instances...")
	}

	if len(m.instances) == 0 {
		// Delightful empty state with project intro
		titleStyle := lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("#7C3AED"))

		title := titleStyle.Render("🎮  mctui")

		tagline := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#A1A1AA")).
			Italic(true).
			Render("A terminal-based Minecraft launcher")

		divider := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#3F3F46")).
			Render("────────────────────────────")

		emptyMsg := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#FAFAFA")).
			MarginTop(1).
			Render("No instances yet. Let's get started!")

		tips := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#626262")).
			MarginTop(1).
			Render(`[n]  Create new instance
[a]  Add Microsoft account
[s]  Settings  •  [q]  Quit`)

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

	var helpText string
	authStatus := "Not logged in"
	if m.accounts != nil {
		if acc := m.accounts.GetActive(); acc != nil {
			authStatus = fmt.Sprintf("Logged in as %s", acc.Name)
		}
	}

	// Build help items based on auth status
	var helpItems []string
	if m.accounts != nil && m.accounts.GetActive() == nil {
		helpItems = []string{"[↵] login & play", "[n] new", "[f] folder", "[d] delete", "[m] mods", "[s] settings", "[a] accounts", "[o] play offline", "[q] quit"}
	} else {
		helpItems = []string{"[↵] launch", "[n] new", "[f] folder", "[d] delete", "[m] mods", "[s] settings", "[a] accounts", "[o] play offline", "[q] quit"}
	}

	// Build help text with smart item-wise wrapping
	helpText = buildHelpText(helpItems, m.width-4)

	help := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#626262")).
		Render(helpText)

	status := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#7C3AED")).
		Padding(1, 0).
		Render(authStatus)

	baseView := lipgloss.JoinVertical(
		lipgloss.Left,
		m.list.View(),
		status,
		help,
	)

	// Show delete confirmation overlay if needed
	if m.confirmDelete && m.deleteTarget != nil {
		titleStyle := lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("#FAFAFA")).
			Background(lipgloss.Color("#EF4444")).
			Padding(0, 1)

		title := titleStyle.Render("⚠️  Delete Instance?")

		msg := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#A1A1AA")).
			Render(fmt.Sprintf("Are you sure you want to delete \"%s\"?\nThis cannot be undone.", m.deleteTarget.Name))

		options := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#626262")).
			MarginTop(1).
			Render("[y] Yes, delete  •  [n] No, cancel")

		promptBox := lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("#EF4444")).
			Padding(1, 2).
			Render(lipgloss.JoinVertical(
				lipgloss.Left,
				title,
				"",
				msg,
				options,
			))

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

// buildHelpText builds help text with item-wise wrapping
// Items are kept together, lines wrap at item boundaries
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
