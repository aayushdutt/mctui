// Package ui wizard provides the new instance creation wizard.
package ui

import (
	"fmt"
	"strings"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
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

	// State
	loading bool
	err     error

	palette CommandPalette

	// help is the `?` read-only key reference overlay (kit_help.go), dismissed by
	// ? / esc / q.
	help bool
}

// helpGroups returns the wizard's bindings grouped for the `?` overlay,
// hand-built to match exactly what Update handles across the three steps
// (version select + filter, loader select, name/starter form) and the footers.
func (m *WizardModel) helpGroups() []HelpGroup {
	return []HelpGroup{
		{Title: "Navigate", Entries: []HelpEntry{
			{"↑↓ / j k", "select"},
			{"tab / ⇧tab", "move between fields"},
		}},
		{Title: "Act", Entries: []HelpEntry{
			{"enter", "next · create"},
			{"space", "toggle snapshots · starter mods"},
			{"/", "filter versions"},
		}},
		{Title: "General", Entries: []HelpEntry{
			{":", "command palette"},
			{"?", "help"},
			{"esc", "back · cancel"},
		}},
	}
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

// NewWizardModel creates a new wizard. showSnapshots seeds the version-list
// snapshot filter from config; toggling it in-wizard persists back via [PersistShowSnapshots].
func NewWizardModel(showSnapshots bool) *WizardModel {
	// Version list
	delegate := ThemeListDelegate(Active.Primary, Active.Secondary)

	vl := list.New([]list.Item{}, delegate, 0, 0)
	vl.Title = "Select Minecraft Version"
	vl.SetShowStatusBar(true)
	vl.SetFilteringEnabled(true)
	vl.Styles.Title = lipgloss.NewStyle().
		Bold(true).
		Foreground(OnColor(Active.Primary)).
		Background(Active.Primary).
		Padding(0, 1)
	ThemeListChrome(&vl)

	// Name input
	ti := textinput.New()
	ti.Placeholder = "My Instance"
	ti.CharLimit = 64
	ti.Width = 40
	ThemeTextInput(&ti)

	m := &WizardModel{
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
		showSnaps:          showSnapshots,
	}
	m.palette = NewCommandPalette("New instance · commands", "Run a command…", m.wizardPaletteItems)
	return m
}

// wizardPaletteItems supplies the wizard's `:` command-palette verbs.
func (m *WizardModel) wizardPaletteItems() []PaletteItem {
	return []PaletteItem{
		{Label: "Cancel (back to home)", Run: func() tea.Cmd {
			return func() tea.Msg { return NavigateToHome{} }
		}},
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

// SetSize updates dimensions. The version step stacks a fair amount of chrome
// around the list — screen header (2) + breadcrumb (2) + section header (2) +
// the list's own status bar, the snapshot checkbox row (2), and the help line
// (2) — so the list viewport is budgeted at height-12 to keep the whole step
// within the terminal (measured in wizard_test.go).
func (m *WizardModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.versionList.SetSize(width-4, max(3, height-12))
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
		if m.palette.Active {
			return m, m.palette.Update(msg)
		}
		// Help overlay is read-only: it swallows every key, and only ? / esc / q
		// dismiss it.
		if m.help {
			switch msg.String() {
			case "?", "esc", "q":
				m.help = false
			}
			return m, nil
		}
		// Space: toggle Fabric starter row (KeySpace and rune " " differ by platform/driver).
		if m.step == StepEnterName && m.selectedLoader == "fabric" &&
			m.nameFormFocus == focusWizardStarterCheckbox &&
			(msg.Type == tea.KeySpace || msg.String() == " " || msg.String() == "space") {
			m.installStarterMods = !m.installStarterMods
			return m, nil
		}
		// Space: toggle the snapshot filter on the version step (visible □ checkbox
		// under the list). Tab is reserved for field/pane cycling everywhere, so the
		// toggle lives on space; guarded against the list's filter input so space
		// still types while filtering.
		if m.step == StepSelectVersion && m.versionList.FilterState() != list.Filtering &&
			(msg.Type == tea.KeySpace || msg.String() == " " || msg.String() == "space") {
			m.showSnaps = !m.showSnaps
			m.updateVersionList("")
			show := m.showSnaps
			return m, func() tea.Msg { return PersistShowSnapshots{Value: show} }
		}
		switch msg.String() {
		case ":":
			// Open the palette unless a text input is receiving keys: the name field
			// on the name step, or the version list's filter box.
			filtering := m.step == StepSelectVersion && m.versionList.FilterState() == list.Filtering
			nameFocused := m.step == StepEnterName && m.nameFormFocus == focusWizardName
			if !filtering && !nameFocused {
				m.palette.Open()
				return m, textinput.Blink
			}
		case "?":
			// Open the read-only key reference — gated exactly like ":" so "?" stays
			// typeable in the name field or the version filter box.
			filtering := m.step == StepSelectVersion && m.versionList.FilterState() == list.Filtering
			nameFocused := m.step == StepEnterName && m.nameFormFocus == focusWizardName
			if !filtering && !nameFocused {
				m.help = true
				return m, nil
			}
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

		case "r":
			// Retry the version-manifest fetch after a load failure.
			if m.err != nil {
				m.err = nil
				m.loading = true
				return m, func() tea.Msg { return RetryLoadVersions{} }
			}

		case "tab":
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
	if err := validateInstanceName(name); err != nil {
		m.nameErr = err.Error()
		m.nameStepSetFocus(focusWizardName)
		return m, textinput.Blink
	}
	m.nameErr = ""

	// ID is intentionally left empty: InstanceManager.Create derives a unique,
	// filesystem-safe folder name from Name. The display Name stays freeform.
	inst := &core.Instance{
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

// panelWidth returns a sensible width for the wizard's bordered panels, clamped
// to the available content width and guarded against a zero/uninitialized shell.
func (m *WizardModel) panelWidth() int {
	w := m.width
	if w <= 0 {
		w = 60
	}
	return min(w, 60)
}

// stepBreadcrumb renders the numbered step progress, e.g.
// "① Version  ▸  ② Loader  ▸  ③ Name". The current step is bold accent,
// completed steps are success-colored, and upcoming steps are muted.
func (m *WizardModel) stepBreadcrumb() string {
	steps := []string{"Version", "Loader", "Name"}
	nums := []string{"①", "②", "③"}
	sep := lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render("  " + GlyphPointer + "  ")

	parts := make([]string, 0, len(steps))
	for i, s := range steps {
		var style lipgloss.Style
		switch {
		case i == int(m.step):
			style = lipgloss.NewStyle().Bold(true).Foreground(Active.Primary)
		case i < int(m.step):
			style = lipgloss.NewStyle().Foreground(Active.Success)
		default:
			style = lipgloss.NewStyle().Foreground(Active.TextMuted)
		}
		label := nums[i] + " " + s
		if i < int(m.step) {
			label = GlyphDone + " " + s
		}
		parts = append(parts, style.Render(label))
	}
	return strings.Join(parts, sep)
}

// View implements tea.Model
func (m *WizardModel) View() string {
	if m.help {
		return boundToScreen(HelpOverlay("New instance", m.helpGroups(), m.width, m.height), m.width, m.height)
	}
	if m.palette.Active {
		return boundToScreen(m.palette.View(m.width, m.height), m.width, m.height)
	}
	if m.err != nil {
		w := m.panelWidth()
		body := lipgloss.NewStyle().
			Foreground(Active.TextSubtle).
			Render(fmt.Sprintf("Couldn't load versions: %v", m.err))
		panel := Panel("Error", body, w, Active.Error)
		hints := KeyHints(w, KeyHint{"r", "retry"}, KeyHint{"esc", "back to home"})
		return lipgloss.JoinVertical(lipgloss.Left, panel, "", hints)
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

	header := ScreenHeader("New Instance", "Pick a version, a loader, and a name")

	return lipgloss.JoinVertical(
		lipgloss.Left,
		header,
		"",
		m.stepBreadcrumb(),
		"",
		content,
	)
}

func (m *WizardModel) viewVersionStep() string {
	w := m.panelWidth()
	if m.loading {
		loading := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Loading versions…")
		return Panel("New Instance", loading, w, Active.Primary)
	}

	// Visible snapshot checkbox under the list — space toggles it (Tab is reserved
	// for field/pane cycling app-wide).
	snapMark := wizardCheckboxGlyph(m.showSnaps, false)
	snapLabel := lipgloss.NewStyle().Foreground(Active.TextDim).Render("Show snapshots")
	snapKey := lipgloss.NewStyle().Foreground(Active.TextFaint).Render("space")
	snapRow := lipgloss.JoinHorizontal(lipgloss.Top, snapMark, " ", snapLabel, "  ", snapKey)

	help := KeyHints(w,
		KeyHint{"↑↓", "select"},
		KeyHint{"space", "snapshots"},
		KeyHint{"enter", "next"},
		KeyHint{"esc", "cancel"},
		KeyHint{"?", "help"},
	)

	return lipgloss.JoinVertical(
		lipgloss.Left,
		SectionHeader("Minecraft version", w),
		"",
		m.versionList.View(),
		"",
		snapRow,
		"",
		help,
	)
}

func (m *WizardModel) viewLoaderStep() string {
	w := m.panelWidth()

	subtitle := lipgloss.NewStyle().
		Foreground(Active.TextDim).
		Render("for Minecraft " + m.selectedVersion)

	var rows []string
	for i, ch := range m.loaderChoices {
		selected := i == m.loaderIndex
		style := lipgloss.NewStyle()
		switch {
		case selected:
			// Selection is the ┃ accent bar (rowMarker) + Primary label — the one
			// selection idiom, not a green ▸ pointer.
			style = style.Bold(true).Foreground(Active.Primary)
		case ch.ComingSoon:
			style = style.Foreground(Active.TextFaint)
		default:
			style = style.Foreground(Active.Text)
		}
		rows = append(rows, rowMarker(selected)+style.Render(ch.Label))
	}

	body := lipgloss.JoinVertical(lipgloss.Left, append([]string{subtitle, ""}, rows...)...)
	panel := Zone("Mod loader", body, w, true)

	hintBlock := ""
	if m.loaderHint != "" {
		hintBlock = lipgloss.NewStyle().
			Foreground(Active.Warning).
			Render(GlyphWarn+" "+m.loaderHint) + "\n\n"
	}

	help := KeyHints(w,
		KeyHint{"↑↓", "select"},
		KeyHint{"enter", "next"},
		KeyHint{"esc", "back"},
		KeyHint{"?", "help"},
	)

	return lipgloss.JoinVertical(
		lipgloss.Left,
		panel,
		"",
		hintBlock+help,
	)
}

func (m *WizardModel) viewNameStep() string {
	w := m.panelWidth()

	summary := lipgloss.NewStyle().
		Foreground(Active.TextSubtle).
		MarginBottom(0).
		Render(fmt.Sprintf("Minecraft %s · %s", m.selectedVersion, m.selectedLoaderLabel))

	nameFocused := m.nameFormFocus == focusWizardName
	nameField := underlineField("Instance name", "", m.nameInput, nameFocused)

	errText := ""
	if m.nameErr != "" {
		errText = lipgloss.NewStyle().
			Foreground(Active.Error).
			MarginTop(1).
			Render(m.nameErr)
	}

	nameBlock := lipgloss.JoinVertical(lipgloss.Left, nameField, errText)

	starterBlock := ""
	if m.selectedLoader == "fabric" {
		cbFocused := m.nameFormFocus == focusWizardStarterCheckbox
		mark := wizardCheckboxGlyph(m.installStarterMods, cbFocused)
		titleLine := lipgloss.NewStyle().Foreground(Active.Title).Render("Install recommended Fabric mods")
		sub := lipgloss.NewStyle().Foreground(Active.TextDim).Render("Fabric API · Mod Menu · Sodium · Lithium")
		labelCol := lipgloss.JoinVertical(lipgloss.Left, titleLine, sub)
		row := lipgloss.JoinHorizontal(lipgloss.Top, mark, "  ", labelCol)
		starterInner := lipgloss.JoinVertical(lipgloss.Left, row)
		// Align □ with the text field prompt: border (1) + inner padding (1) = 2 cells.
		rowStyle := lipgloss.NewStyle().PaddingLeft(2)
		if cbFocused {
			rowStyle = lipgloss.NewStyle().
				Border(lipgloss.NormalBorder(), false, false, false, true).
				BorderForeground(Active.Primary).
				Background(Active.BorderFaint).
				PaddingLeft(1).
				PaddingRight(1)
		}
		starterBlock = rowStyle.Render(starterInner)
	}

	createBtn := Pill("Create", m.nameFormFocus == focusWizardSubmit)
	buttonRow := lipgloss.NewStyle().MarginTop(1).Render(createBtn)

	bodyParts := []string{summary, "", nameBlock}
	if starterBlock != "" {
		bodyParts = append(bodyParts, starterBlock)
	}
	bodyParts = append(bodyParts, buttonRow)
	body := lipgloss.JoinVertical(lipgloss.Left, bodyParts...)
	panel := Zone("Name your instance", body, w, true)

	hints := []KeyHint{
		{"tab/↑↓", "move"},
		{"enter", "create"},
		{"esc", "back"},
	}
	if m.selectedLoader == "fabric" {
		hints = append(hints, KeyHint{"space", "toggle mods"})
	}
	hints = append(hints, KeyHint{"?", "help"})
	help := KeyHints(w, hints...)

	return lipgloss.JoinVertical(
		lipgloss.Left,
		panel,
		"",
		help,
	)
}

// wizardCheckboxGlyph uses □ / ■ — one monospace cell each, so the glyph is always square in the grid.
// Lipgloss borders + Width/Height do not reliably map to equal row/column counts, which caused tall boxes.
func wizardCheckboxGlyph(checked, focused bool) string {
	// Checkboxes live in the focus/selection domain, so an on/focused box reads in
	// the accent (Primary), never green — Success/Warning are reserved for state.
	if checked {
		return lipgloss.NewStyle().Foreground(Active.Primary).Render("■")
	}
	st := lipgloss.NewStyle().Foreground(Active.TextMuted)
	if focused {
		st = st.Foreground(Active.Primary)
	}
	return st.Render("□")
}

// validateInstanceName validates the freeform display name only. Filesystem safety
// and uniqueness are handled when the folder/ID is derived (see core.SanitizeInstanceDirName
// and InstanceManager.Create), so names may contain path characters and may duplicate.
func validateInstanceName(name string) error {
	if strings.TrimSpace(name) == "" {
		return fmt.Errorf("Name cannot be empty")
	}
	for _, r := range name {
		if r < 0x20 {
			return fmt.Errorf("Name contains control characters")
		}
	}
	return nil
}
