// Package ui wizard provides the new instance creation wizard.
package ui

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/google/uuid"
	"github.com/quasar/mctui/internal/core"
)

// WizardStep represents the current wizard step
type WizardStep int

const (
	StepSelectVersion WizardStep = iota
	StepSelectLoader
	StepEnterName
)

// WizardModel is the new instance wizard
type WizardModel struct {
	step   WizardStep
	width  int
	height int

	// Version selection
	versionList list.Model
	versions    []core.Version
	showSnaps   bool

	// Loader selection
	selectedVersion string
	loaderIndex     int
	loaders         []string

	// Name input
	nameInput      textinput.Model
	selectedLoader string

	// State
	loading bool
	err     error
}

// versionItem for the list
type versionItem struct {
	version core.Version
	latest  bool
}

func (i versionItem) Title() string {
	title := i.version.ID
	if i.latest {
		title += " ★"
	}
	return title
}
func (i versionItem) Description() string {
	return fmt.Sprintf("%s • %s", i.version.Type, i.version.ReleaseTime.Format("Jan 2006"))
}
func (i versionItem) FilterValue() string { return i.version.ID }

// NewWizardModel creates a new wizard
func NewWizardModel() *WizardModel {
	// Version list
	delegate := list.NewDefaultDelegate()
	delegate.Styles.SelectedTitle = delegate.Styles.SelectedTitle.
		Foreground(lipgloss.Color("#10B981")).
		BorderLeftForeground(lipgloss.Color("#10B981"))
	delegate.Styles.SelectedDesc = delegate.Styles.SelectedDesc.
		Foreground(lipgloss.Color("#6EE7B7")).
		BorderLeftForeground(lipgloss.Color("#10B981"))

	vl := list.New([]list.Item{}, delegate, 0, 0)
	vl.Title = "Select Minecraft Version"
	vl.SetShowStatusBar(true)
	vl.SetFilteringEnabled(true)
	vl.Styles.Title = lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FAFAFA")).
		Background(lipgloss.Color("#10B981")).
		Padding(0, 1)

	// Name input
	ti := textinput.New()
	ti.Placeholder = "My Instance"
	ti.CharLimit = 64
	ti.Width = 40

	return &WizardModel{
		step:        StepSelectVersion,
		versionList: vl,
		loaders:     []string{"Vanilla", "Fabric", "Forge", "Quilt"},
		nameInput:   ti,
		loading:     true,
	}
}

// SetVersions updates the version list
func (m *WizardModel) SetVersions(versions []core.Version, latest string) {
	m.versions = versions
	m.loading = false
	m.updateVersionList(latest)
}

func (m *WizardModel) updateVersionList(latest string) {
	var items []list.Item
	for _, v := range m.versions {
		if !m.showSnaps && v.Type != core.VersionTypeRelease {
			continue
		}
		items = append(items, versionItem{
			version: v,
			latest:  v.ID == latest,
		})
	}
	m.versionList.SetItems(items)
}

// SetSize updates dimensions
func (m *WizardModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.versionList.SetSize(width-4, height-8)
}

// Init implements tea.Model
func (m *WizardModel) Init() tea.Cmd {
	return textinput.Blink
}

// Update implements tea.Model
func (m *WizardModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case VersionsLoaded:
		if msg.Error != nil {
			m.err = msg.Error
			return m, nil
		}
		m.SetVersions(msg.Versions, msg.Latest)
		return m, nil

	case tea.KeyMsg:
		switch msg.String() {
		case "esc":
			if m.step > StepSelectVersion {
				m.step--
				return m, nil
			}
			return m, func() tea.Msg { return NavigateToHome{} }

		case "tab":
			if m.step == StepSelectVersion {
				m.showSnaps = !m.showSnaps
				m.updateVersionList("")
			}
			return m, nil

		case "enter":
			return m.handleEnter()

		case "up", "k":
			if m.step == StepSelectLoader && m.loaderIndex > 0 {
				m.loaderIndex--
			}
		case "down", "j":
			if m.step == StepSelectLoader && m.loaderIndex < len(m.loaders)-1 {
				m.loaderIndex++
			}
		}
	}

	// Delegate to sub-components
	var cmd tea.Cmd
	switch m.step {
	case StepSelectVersion:
		if m.versionList.FilterState() == list.Filtering {
			m.versionList, cmd = m.versionList.Update(msg)
			return m, cmd
		}
		m.versionList, cmd = m.versionList.Update(msg)
	case StepEnterName:
		m.nameInput, cmd = m.nameInput.Update(msg)
	}

	return m, cmd
}

func (m *WizardModel) handleEnter() (*WizardModel, tea.Cmd) {
	switch m.step {
	case StepSelectVersion:
		if item, ok := m.versionList.SelectedItem().(versionItem); ok {
			m.selectedVersion = item.version.ID
			m.step = StepSelectLoader
		}
	case StepSelectLoader:
		m.selectedLoader = strings.ToLower(m.loaders[m.loaderIndex])
		m.step = StepEnterName
		m.nameInput.SetValue(fmt.Sprintf("%s %s", m.selectedVersion, m.loaders[m.loaderIndex]))
		m.nameInput.Focus()
	case StepEnterName:
		name := m.nameInput.Value()
		if name == "" {
			name = "New Instance"
		}

		inst := &core.Instance{
			ID:         uuid.New().String()[:8],
			Name:       name,
			Version:    m.selectedVersion,
			Loader:     m.selectedLoader,
			LastPlayed: time.Time{},
		}

		return m, func() tea.Msg { return InstanceCreated{Instance: inst} }
	}
	return m, nil
}

// View implements tea.Model
func (m *WizardModel) View() string {
	if m.err != nil {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#EF4444")).
			Render(fmt.Sprintf("Error: %v\n\nPress Esc to go back", m.err))
	}

	var content string
	switch m.step {
	case StepSelectVersion:
		content = m.viewVersionStep()
	case StepSelectLoader:
		content = m.viewLoaderStep()
	case StepEnterName:
		content = m.viewNameStep()
	}

	// Progress indicator
	steps := []string{"Version", "Loader", "Name"}
	var progress strings.Builder
	for i, s := range steps {
		style := lipgloss.NewStyle().Foreground(lipgloss.Color("#626262"))
		if i == int(m.step) {
			style = style.Bold(true).Foreground(lipgloss.Color("#10B981"))
		} else if i < int(m.step) {
			style = style.Foreground(lipgloss.Color("#10B981"))
		}
		if i > 0 {
			progress.WriteString(" → ")
		}
		progress.WriteString(style.Render(s))
	}

	header := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FAFAFA")).
		Render("New Instance")

	return lipgloss.JoinVertical(
		lipgloss.Left,
		header,
		progress.String(),
		"",
		content,
	)
}

func (m *WizardModel) viewVersionStep() string {
	if m.loading {
		return "Loading versions..."
	}

	snapsToggle := "[Tab] Show snapshots: "
	if m.showSnaps {
		snapsToggle += "ON"
	} else {
		snapsToggle += "OFF"
	}

	help := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#626262")).
		Render(snapsToggle + " • [Enter] Select • [Esc] Cancel")

	return lipgloss.JoinVertical(
		lipgloss.Left,
		m.versionList.View(),
		help,
	)
}

func (m *WizardModel) viewLoaderStep() string {
	title := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FAFAFA")).
		Render(fmt.Sprintf("Select Mod Loader for %s", m.selectedVersion))

	var loaderList strings.Builder
	for i, loader := range m.loaders {
		style := lipgloss.NewStyle().Padding(0, 2)
		if i == m.loaderIndex {
			style = style.
				Bold(true).
				Foreground(lipgloss.Color("#10B981")).
				SetString("▸ " + loader)
		} else {
			style = style.SetString("  " + loader)
		}
		loaderList.WriteString(style.Render())
		loaderList.WriteString("\n")
	}

	help := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#626262")).
		Render("[↑↓] Select • [Enter] Next • [Esc] Back")

	return lipgloss.JoinVertical(
		lipgloss.Left,
		title,
		"",
		loaderList.String(),
		"",
		help,
	)
}

func (m *WizardModel) viewNameStep() string {
	title := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FAFAFA")).
		Render("Name Your Instance")

	summary := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#A1A1AA")).
		Render(fmt.Sprintf("Minecraft %s • %s", m.selectedVersion, m.loaders[m.loaderIndex]))

	inputStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#10B981")).
		Padding(0, 1)

	help := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#626262")).
		Render("[Enter] Create • [Esc] Back")

	return lipgloss.JoinVertical(
		lipgloss.Left,
		title,
		summary,
		"",
		inputStyle.Render(m.nameInput.View()),
		"",
		help,
	)
}
