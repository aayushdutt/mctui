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
}

type homeKeyMap struct {
	Launch   key.Binding
	NewInst  key.Binding
	Mods     key.Binding
	Settings key.Binding
	Delete   key.Binding
}

func defaultHomeKeyMap() homeKeyMap {
	return homeKeyMap{
		Launch: key.NewBinding(
			key.WithKeys("enter", "l"),
			key.WithHelp("enter", "launch"),
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
		// Don't handle keys if filtering
		if m.list.FilterState() == list.Filtering {
			break
		}

		switch {
		case key.Matches(msg, m.keys.Launch):
			if inst := m.SelectedInstance(); inst != nil {
				return m, func() tea.Msg { return NavigateToLaunch{Instance: inst} }
			}
		case key.Matches(msg, m.keys.NewInst):
			return m, func() tea.Msg { return NavigateToNewInstance{} }
		case key.Matches(msg, m.keys.Mods):
			if inst := m.SelectedInstance(); inst != nil {
				return m, func() tea.Msg { return NavigateToMods{Instance: inst} }
			}
		case key.Matches(msg, m.keys.Settings):
			return m, func() tea.Msg { return NavigateToSettings{} }
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
			Render("\n\n[n] new instance • [s] settings • [q] quit")

		return lipgloss.JoinVertical(
			lipgloss.Left,
			m.list.View(),
			empty,
			help,
		)
	}

	help := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#626262")).
		Render("[enter] launch • [n] new • [m] mods • [s] settings • [q] quit")

	return lipgloss.JoinVertical(
		lipgloss.Left,
		m.list.View(),
		help,
	)
}
