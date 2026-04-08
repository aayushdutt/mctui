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
	"github.com/aayushdutt/mctui/internal/core"
)

// WizardStep represents the current wizard step
type WizardStep int

const (
	StepSelectVersion WizardStep = iota
	StepSelectLoader
	StepEnterName
)

// nameFormFocus is which control is active on the name step.
type nameFormFocus int

const (
	focusWizardName            nameFormFocus = iota
	focusWizardStarterCheckbox               // Fabric only (skipped in focus order when not Fabric)
	focusWizardSubmit
)

// loaderChoice is one row in the mod loader step (extensible for future loaders).
type loaderChoice struct {
	Label      string
	ID         string // persisted to Instance.Loader when not ComingSoon
	ComingSoon bool
}

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
	selectedVersion     string
	loaderIndex         int
	loaderChoices       []loaderChoice
	selectedLoader      string // instance loader id: vanilla, fabric
	selectedLoaderLabel string // display label for summary
	loaderHint          string // e.g. coming-soon message

	// Name input
	nameInput textinput.Model
	nameErr   string

	// Fabric only: opt-in starter mods (Fabric API, Mod Menu, Sodium, Lithium) after create.
	installStarterMods bool
	nameFormFocus      nameFormFocus

	existingNames map[string]struct{}

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
func NewWizardModel(instances []*core.Instance) *WizardModel {
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
	ti.PromptStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("#71717A"))
	ti.TextStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("#FAFAFA"))

	existingNames := make(map[string]struct{})
	for _, inst := range instances {
		existingNames[strings.ToLower(inst.Name)] = struct{}{}
	}

	return &WizardModel{
		step:        StepSelectVersion,
		versionList: vl,
		// loaderIndex 0 = Fabric (first row; Vanilla below)
		loaderChoices: []loaderChoice{
			{Label: "Fabric", ID: "fabric"},
			{Label: "Vanilla", ID: "vanilla"},
			{Label: "Forge (coming soon)", ID: "", ComingSoon: true},
			{Label: "Quilt (coming soon)", ID: "", ComingSoon: true},
		},
		nameInput:          ti,
		installStarterMods: true,
		loading:            true,
		existingNames:      existingNames,
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
		// Space: toggle Fabric starter row (KeySpace and rune " " differ by platform/driver).
		if m.step == StepEnterName && m.selectedLoader == "fabric" &&
			m.nameFormFocus == focusWizardStarterCheckbox &&
			(msg.Type == tea.KeySpace || msg.String() == " " || msg.String() == "space") {
			m.installStarterMods = !m.installStarterMods
			return m, nil
		}
		switch msg.String() {
		case "esc":
			if m.step > StepSelectVersion {
				if m.step == StepEnterName {
					m.nameErr = ""
					m.nameStepSetFocus(focusWizardName)
				}
				m.step--
				return m, nil
			}
			return m, func() tea.Msg { return NavigateToHome{} }

		case "tab":
			if m.step == StepSelectVersion {
				m.showSnaps = !m.showSnaps
				m.updateVersionList("")
				return m, nil
			}
			if m.step == StepEnterName {
				m.nameStepCycleFocus(1)
				return m, textinput.Blink
			}
			return m, nil

		case "shift+tab":
			if m.step == StepEnterName {
				m.nameStepCycleFocus(-1)
				return m, textinput.Blink
			}

		case "enter":
			if m.step == StepEnterName {
				return m.handleNameStepEnter()
			}
			return m.handleEnter()

		case "up":
			if m.step == StepSelectLoader {
				m.loaderHint = ""
				m.moveLoaderSelection(-1)
			} else if m.step == StepEnterName {
				m.nameStepCycleFocus(-1)
				return m, textinput.Blink
			}
		case "down":
			if m.step == StepSelectLoader {
				m.loaderHint = ""
				m.moveLoaderSelection(1)
			} else if m.step == StepEnterName {
				m.nameStepCycleFocus(1)
				return m, textinput.Blink
			}
		case "k":
			if m.step == StepSelectLoader {
				m.loaderHint = ""
				m.moveLoaderSelection(-1)
			}
		case "j":
			if m.step == StepSelectLoader {
				m.loaderHint = ""
				m.moveLoaderSelection(1)
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
		if m.nameFormFocus == focusWizardName {
			m.nameInput, cmd = m.nameInput.Update(msg)
		}
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
		ch := m.loaderChoices[m.loaderIndex]
		if ch.ComingSoon {
			m.loaderHint = "That loader isn't available yet — choose Vanilla or Fabric."
			return m, nil
		}
		m.selectedLoader = ch.ID
		m.selectedLoaderLabel = ch.Label
		m.loaderHint = ""
		m.installStarterMods = ch.ID == "fabric"
		m.nameStepSetFocus(focusWizardName)
		m.step = StepEnterName
		m.nameInput.SetValue(fmt.Sprintf("%s %s", m.selectedVersion, ch.Label))
		m.nameInput.Focus()
	}
	return m, nil
}

func (m *WizardModel) nameStepFocusOrder() []nameFormFocus {
	if m.selectedLoader == "fabric" {
		return []nameFormFocus{
			focusWizardName,
			focusWizardStarterCheckbox,
			focusWizardSubmit,
		}
	}
	return []nameFormFocus{focusWizardName, focusWizardSubmit}
}

func (m *WizardModel) nameStepSetFocus(f nameFormFocus) {
	m.nameFormFocus = f
	if f == focusWizardName {
		m.nameInput.Focus()
	} else {
		m.nameInput.Blur()
	}
}

func (m *WizardModel) nameStepCycleFocus(delta int) {
	order := m.nameStepFocusOrder()
	if len(order) == 0 {
		return
	}
	idx := 0
	found := false
	for i, focus := range order {
		if focus == m.nameFormFocus {
			idx = i
			found = true
			break
		}
	}
	if !found {
		m.nameStepSetFocus(order[0])
		return
	}
	n := len(order)
	next := (idx + delta + n) % n
	m.nameStepSetFocus(order[next])
}

func (m *WizardModel) handleNameStepEnter() (*WizardModel, tea.Cmd) {
	switch m.nameFormFocus {
	case focusWizardStarterCheckbox:
		m.installStarterMods = !m.installStarterMods
		return m, nil
	case focusWizardName, focusWizardSubmit:
		return m.submitNameStep()
	default:
		return m, nil
	}
}

func (m *WizardModel) submitNameStep() (*WizardModel, tea.Cmd) {
	name := strings.TrimSpace(m.nameInput.Value())
	if name == "" {
		name = "New Instance"
	}
	if err := validateInstanceName(name, m.existingNames); err != nil {
		m.nameErr = err.Error()
		m.nameStepSetFocus(focusWizardName)
		return m, textinput.Blink
	}
	m.nameErr = ""

	inst := &core.Instance{
		ID:                       name,
		Name:                     name,
		Version:                  m.selectedVersion,
		Loader:                   m.selectedLoader,
		LastPlayed:               time.Time{},
		InstallStarterFabricMods: m.installStarterMods && m.selectedLoader == "fabric",
	}

	return m, func() tea.Msg {
		return InstanceCreated{
			Instance:                 inst,
			InstallStarterFabricMods: inst.InstallStarterFabricMods,
		}
	}
}

// moveLoaderSelection cycles only among loaders that are not ComingSoon (delta +1 = down, -1 = up).
func (m *WizardModel) moveLoaderSelection(delta int) {
	var selectable []int
	for i, ch := range m.loaderChoices {
		if !ch.ComingSoon {
			selectable = append(selectable, i)
		}
	}
	if len(selectable) == 0 {
		return
	}
	cur := -1
	for i, ix := range selectable {
		if ix == m.loaderIndex {
			cur = i
			break
		}
	}
	if cur < 0 {
		m.loaderIndex = selectable[0]
		return
	}
	n := len(selectable)
	next := (cur + delta + n) % n
	m.loaderIndex = selectable[next]
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
	for i, ch := range m.loaderChoices {
		style := lipgloss.NewStyle().Padding(0, 2)
		label := ch.Label
		prefix := "  "
		if i == m.loaderIndex {
			prefix = "▸ "
			style = style.Bold(true).Foreground(lipgloss.Color("#10B981"))
		} else if ch.ComingSoon {
			style = style.Foreground(lipgloss.Color("#626262"))
		}
		style = style.SetString(prefix + label)
		loaderList.WriteString(style.Render())
		loaderList.WriteString("\n")
	}

	hintBlock := ""
	if m.loaderHint != "" {
		hintBlock = lipgloss.NewStyle().
			Foreground(lipgloss.Color("#FBBF24")).
			Render(m.loaderHint) + "\n\n"
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
		hintBlock+help,
	)
}

func (m *WizardModel) viewNameStep() string {
	title := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FAFAFA")).
		MarginBottom(0).
		Render("Name your instance")

	summary := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#A1A1AA")).
		MarginBottom(0).
		Render(fmt.Sprintf("Minecraft %s · %s", m.selectedVersion, m.selectedLoaderLabel))

	nameFocused := m.nameFormFocus == focusWizardName
	nameBorder := lipgloss.Color("#3F3F46")
	if nameFocused {
		nameBorder = lipgloss.Color("#10B981")
	}
	fieldLabel := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#71717A")).
		MarginBottom(0).
		Render("Instance name")

	inputStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(nameBorder).
		Padding(0, 1)

	errText := ""
	if m.nameErr != "" {
		errText = lipgloss.NewStyle().
			Foreground(lipgloss.Color("#EF4444")).
			MarginTop(1).
			Render(m.nameErr)
	}

	nameBlock := lipgloss.JoinVertical(
		lipgloss.Left,
		fieldLabel,
		inputStyle.Render(m.nameInput.View()),
		errText,
	)

	starterBlock := ""
	if m.selectedLoader == "fabric" {
		cbFocused := m.nameFormFocus == focusWizardStarterCheckbox
		mark := wizardCheckboxGlyph(m.installStarterMods, cbFocused)
		titleLine := lipgloss.NewStyle().Foreground(lipgloss.Color("#E4E4E7")).Render("Install recommended Fabric mods")
		sub := lipgloss.NewStyle().Foreground(lipgloss.Color("#71717A")).Render("Fabric API · Mod Menu · Sodium · Lithium")
		labelCol := lipgloss.JoinVertical(lipgloss.Left, titleLine, sub)
		row := lipgloss.JoinHorizontal(lipgloss.Top, mark, "  ", labelCol)
		starterInner := lipgloss.JoinVertical(lipgloss.Left, row)
		// Align □ with the text field prompt: border (1) + inner padding (1) = 2 cells.
		rowStyle := lipgloss.NewStyle().PaddingLeft(2)
		if cbFocused {
			rowStyle = lipgloss.NewStyle().
				Border(lipgloss.NormalBorder(), false, false, false, true).
				BorderForeground(lipgloss.Color("#10B981")).
				Background(lipgloss.Color("#27272A")).
				PaddingLeft(1).
				PaddingRight(1)
		}
		starterBlock = rowStyle.Render(starterInner)
	}

	// Match vertical rhythm to the gap below the bordered name field → checkbox:
	// the input box adds visual weight, so we add explicit space before Create (and help).
	formSectionGap := 1

	createBtn := wizardFormButton("Create", m.nameFormFocus == focusWizardSubmit, true)
	buttonRow := lipgloss.NewStyle().MarginTop(formSectionGap).Render(createBtn)

	help := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#52525B")).
		MarginTop(formSectionGap).
		Render(helpTextNameStep(m.selectedLoader == "fabric"))

	return lipgloss.JoinVertical(
		lipgloss.Left,
		title,
		summary,
		nameBlock,
		starterBlock,
		buttonRow,
		help,
	)
}

// wizardCheckboxGlyph uses □ / ■ — one monospace cell each, so the glyph is always square in the grid.
// Lipgloss borders + Width/Height do not reliably map to equal row/column counts, which caused tall boxes.
func wizardCheckboxGlyph(checked, focused bool) string {
	if checked {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#34D399")).
			Render("■")
	}
	st := lipgloss.NewStyle().Foreground(lipgloss.Color("#52525B"))
	if focused {
		st = st.Foreground(lipgloss.Color("#10B981"))
	}
	return st.Render("□")
}

func wizardFormButton(label string, focused, primary bool) string {
	st := lipgloss.NewStyle().
		Padding(0, 2).
		Border(lipgloss.RoundedBorder())
	if primary {
		if focused {
			st = st.BorderForeground(lipgloss.Color("#10B981")).Foreground(lipgloss.Color("#FAFAFA")).Bold(true)
		} else {
			st = st.BorderForeground(lipgloss.Color("#3F3F46")).Foreground(lipgloss.Color("#A1A1AA"))
		}
	} else {
		if focused {
			st = st.BorderForeground(lipgloss.Color("#71717A")).Foreground(lipgloss.Color("#E4E4E7"))
		} else {
			st = st.BorderForeground(lipgloss.Color("#27272A")).Foreground(lipgloss.Color("#71717A"))
		}
	}
	return st.Render(label)
}

func helpTextNameStep(fabric bool) string {
	base := "[Tab] / [Shift+Tab] / [↑][↓] move · [Enter] create · [Esc] back to loader"
	if fabric {
		return base + " · [Space] toggle mods option"
	}
	return base
}

func validateInstanceName(name string, existingNames map[string]struct{}) error {
	if name == "." || name == ".." {
		return fmt.Errorf("Name cannot be '.' or '..'")
	}
	if strings.ContainsAny(name, "<>:\"/\\|?*") {
		return fmt.Errorf("Name contains invalid characters")
	}
	if strings.HasSuffix(name, " ") || strings.HasSuffix(name, ".") {
		return fmt.Errorf("Name cannot end with a space or period")
	}
	for _, r := range name {
		if r < 32 {
			return fmt.Errorf("Name contains invalid characters")
		}
	}

	key := strings.ToLower(name)
	if _, ok := existingNames[key]; ok {
		return fmt.Errorf("Name already exists")
	}

	return nil
}
