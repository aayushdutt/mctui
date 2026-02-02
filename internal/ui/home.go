// Package ui contains all TUI view components.
// Each view is a Bubbletea model that can be composed into the main app.
package ui

import (
	"fmt"
	"sort"
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

	// Auth prompt state
	showAuthPrompt    bool
	selectedForLaunch *core.Instance
	promptSelection   int // 0 = Login, 1 = Play Offline
}

type homeKeyMap struct {
	Launch      key.Binding
	PlayOffline key.Binding
	NewInst     key.Binding
	Mods     key.Binding
	Settings key.Binding
	Delete   key.Binding
	Auth     key.Binding
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
	m.list.SetSize(width, height-3)
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
		// Handle auth prompt mode
		if m.showAuthPrompt {
			switch msg.String() {
			case "up", "k", "left", "h":
				m.promptSelection = 0
			case "down", "j", "right", "l":
				m.promptSelection = 1
			case "enter":
				m.showAuthPrompt = false
				if m.promptSelection == 0 {
					// Login first
					return m, func() tea.Msg { return NavigateToAuth{} }
				}
				// Play offline
				inst := m.selectedForLaunch
				m.selectedForLaunch = nil
				return m, func() tea.Msg { return NavigateToLaunch{Instance: inst, Offline: true} }
			case "esc", "q":
				m.showAuthPrompt = false
				m.selectedForLaunch = nil
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
				// If authenticated, launch online. Else show prompt.
				if m.accounts != nil && m.accounts.GetActive() != nil {
					return m, func() tea.Msg { return NavigateToLaunch{Instance: inst, Offline: false} }
				}
				// Not authenticated - show prompt
				m.showAuthPrompt = true
				m.selectedForLaunch = inst
				m.promptSelection = 0
				return m, nil
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
		empty := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#A1A1AA")).
			Render("No instances yet. Press 'n' to create one.")

		help := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#626262")).
			Render("\n\n[n] new instance • [s] settings • [a] accounts • [q] quit")

		return lipgloss.JoinVertical(
			lipgloss.Left,
			m.list.View(),
			empty,
			help,
		)
	}

	var helpText string
	authStatus := "Not logged in"
	if m.accounts != nil {
		if acc := m.accounts.GetActive(); acc != nil {
			authStatus = fmt.Sprintf("Logged in as %s", acc.Name)
		}
	}

	if m.accounts != nil && m.accounts.GetActive() == nil {
		helpText = "[enter] login & play • [n] new • [m] mods • [s] settings • [a] accounts • [o] play offline • [q] quit"
	} else {
		helpText = "[enter] launch • [n] new • [m] mods • [s] settings • [a] accounts • [o] play offline • [q] quit"
	}

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

	// Show auth prompt overlay if needed
	if m.showAuthPrompt {
		// Build prompt box
		titleStyle := lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("#FAFAFA")).
			Background(lipgloss.Color("#EF4444")).
			Padding(0, 1)

		title := titleStyle.Render("⚠️  Not Authenticated")

		instName := ""
		if m.selectedForLaunch != nil {
			instName = m.selectedForLaunch.Name
		}
		subtitle := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#A1A1AA")).
			Render(fmt.Sprintf("To play \"%s\", choose an option:", instName))

		// Options
		options := []string{"🔐 Log in with Microsoft", "🎮 Play Offline"}
		var optionsView string
		for i, opt := range options {
			cursor := "  "
			style := lipgloss.NewStyle().Foreground(lipgloss.Color("#A1A1AA"))
			if i == m.promptSelection {
				cursor = "▸ "
				style = lipgloss.NewStyle().Foreground(lipgloss.Color("#7C3AED")).Bold(true)
			}
			optionsView += style.Render(cursor+opt) + "\n"
		}

		helpPrompt := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#626262")).
			Render("[↑/↓] Select • [Enter] Confirm • [Esc] Cancel")

		promptBox := lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("#EF4444")).
			Padding(1, 2).
			Render(lipgloss.JoinVertical(
				lipgloss.Left,
				title,
				"",
				subtitle,
				"",
				optionsView,
				helpPrompt,
			))

		// Overlay the prompt on top
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
