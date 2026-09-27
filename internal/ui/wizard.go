// Package ui wizard provides the new instance creation wizard.
package ui

import (
	"fmt"
	"io"
	"strings"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
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
	latest      string // latest release id, marked in the list
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
	autoName  string // the name the wizard suggested; replaced on back/forward only if unedited

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
			{"tab / ⇧tab", "move between fields (name step)"},
		}},
		{Title: "Act", Entries: []HelpEntry{
			{"enter", "next · create"},
			{"space", "show snapshots · toggle starter mods"},
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

func (i versionItem) Title() string       { return i.version.ID }
func (i versionItem) Description() string { return i.version.ReleaseTime.Format("Jan 2006") }
func (i versionItem) FilterValue() string { return i.version.ID }

// versionDelegate renders the version list one line per version:
//
//	› 1.21.4      latest       Dec 2024
//	  25w14a      snapshot     Apr 2025
//
// the selected row with the › pointer and a bright id, "latest" in green, a
// non-release type dim, and the release month dim.
type versionDelegate struct{}

func (versionDelegate) Height() int                         { return 1 }
func (versionDelegate) Spacing() int                        { return 0 }
func (versionDelegate) Update(tea.Msg, *list.Model) tea.Cmd { return nil }
func (versionDelegate) Render(w io.Writer, m list.Model, index int, item list.Item) {
	vi, ok := item.(versionItem)
	if !ok {
		return
	}
	selected := index == m.Index() && m.FilterState() != list.Filtering
	id := lipgloss.NewStyle().Foreground(Active.Text)
	if selected {
		id = id.Bold(true).Foreground(Active.TextStrong)
	}
	tag := ""
	switch {
	case vi.latest:
		tag = lipgloss.NewStyle().Foreground(Active.Success).Render("latest")
	case vi.version.Type != core.VersionTypeRelease:
		tag = lipgloss.NewStyle().Foreground(Active.TextMuted).Render(strings.ReplaceAll(string(vi.version.Type), "_", " "))
	}
	cell := func(s string, w int) string { return s + strings.Repeat(" ", max(1, w-lipgloss.Width(s))) }
	date := lipgloss.NewStyle().Foreground(Active.TextDim).Render(vi.version.ReleaseTime.Format("Jan 2006"))
	line := rowMarker(selected) + cell(id.Render(ansi.Truncate(vi.version.ID, 18, "…")), 20) + cell(tag, 13) + date
	_, _ = fmt.Fprint(w, ansi.Truncate(line, m.Width(), ""))
}

// NewWizardModel creates a new wizard. showSnapshots seeds the version-list
// snapshot filter from config; toggling it in-wizard persists back via [PersistShowSnapshots].
func NewWizardModel(showSnapshots bool) *WizardModel {
	// Version list: one line per version, no built-in title/help/status chrome
	// (the step has its own), quit keys off (q must not quit the app from here),
	// paging on PgUp/PgDn only like everywhere else.
	vl := list.New([]list.Item{}, versionDelegate{}, 0, 0)
	vl.SetShowTitle(false)
	vl.SetShowHelp(false)
	vl.SetShowStatusBar(false)
	vl.SetFilteringEnabled(true)
	vl.DisableQuitKeybindings()
	vl.KeyMap.PrevPage.SetKeys("pgup")
	vl.KeyMap.NextPage.SetKeys("pgdown")
	vl.SetStatusBarItemName("version", "versions")
	ThemeListChrome(&vl)

	// Name input
	ti := formInput("", "My Instance")
	ti.CharLimit = 64

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
	m.latest = latest
	m.loading = false
	m.updateVersionList()
}

// updateVersionList rebuilds the list for the snapshot filter, keeping the
// cursor on the same version where it's still listed. The returned command
// re-applies an active text filter (callers must run it).
func (m *WizardModel) updateVersionList() tea.Cmd {
	prev := ""
	if it, ok := m.versionList.SelectedItem().(versionItem); ok {
		prev = it.version.ID
	}
	var items []list.Item
	sel := 0
	for _, v := range m.versions {
		if !m.showSnaps && v.Type != core.VersionTypeRelease {
			continue
		}
		if v.ID == prev {
			sel = len(items)
		}
		items = append(items, versionItem{version: v, latest: v.ID == m.latest})
	}
	cmd := m.versionList.SetItems(items)
	// Re-measure: bubbles sizes a page before it knows the list has more than
	// one, and with no gap between rows the page dots then take an extra line —
	// a view one row taller than the screen until the next resize.
	m.versionList.SetSize(m.versionList.Width(), m.versionList.Height())
	if m.versionList.FilterState() == list.Unfiltered {
		m.versionList.Select(sel)
	}
	return cmd
}

// SetSize updates dimensions. The version step stacks header (2) + step trail
// (2) + pane title and rule (2) around the list, and a blank + footer (2) below.
func (m *WizardModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.versionList.SetSize(m.panelWidth(), max(3, height-8))
	m.nameInput.Width = formValueWidth(m.panelWidth())
}

// Init implements tea.Model
func (m *WizardModel) Init() tea.Cmd {
	return textinput.Blink
}

// Update implements tea.Model
func (m *WizardModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.MouseMsg:
		if m.step == StepSelectVersion && !m.help && !m.palette.Active {
			switch msg.Button {
			case tea.MouseButtonWheelUp:
				m.versionList.CursorUp()
			case tea.MouseButtonWheelDown:
				m.versionList.CursorDown()
			}
		}
		return m, nil

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
			show := m.showSnaps
			return m, tea.Batch(m.updateVersionList(), func() tea.Msg { return PersistShowSnapshots{Value: show} })
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
			if m.step == StepSelectVersion && m.versionList.FilterState() != list.Unfiltered {
				// Esc clears the version filter first (the list handles it).
				var cmd tea.Cmd
				m.versionList, cmd = m.versionList.Update(msg)
				return m, cmd
			}
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
			if m.step == StepSelectVersion && m.versionList.FilterState() == list.Filtering {
				// Enter applies the filter; a second Enter picks the version.
				var cmd tea.Cmd
				m.versionList, cmd = m.versionList.Update(msg)
				return m, cmd
			}
			if m.step == StepEnterName {
				return m.handleNameStepEnter()
			}
			return m.handleEnter()

		case "up", "k", "down", "j":
			// ↑/↓ (and j/k where they don't type) move and stop at the ends.
			dir := 1
			if msg.String() == "up" || msg.String() == "k" {
				dir = -1
			}
			typing := m.step == StepEnterName && m.nameFormFocus == focusWizardName
			isLetter := msg.String() == "j" || msg.String() == "k"
			switch {
			case m.step == StepSelectLoader:
				m.loaderHint = ""
				m.moveLoaderSelection(dir)
				return m, nil
			case m.step == StepEnterName && !(typing && isLetter):
				m.nameStepMoveFocus(dir)
				return m, textinput.Blink
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
		if ch.ID != m.selectedLoader {
			// Starter mods default on for Fabric; keep the user's choice when they
			// step back and forward without changing the loader.
			m.installStarterMods = ch.ID == "fabric"
		}
		m.selectedLoader = ch.ID
		m.selectedLoaderLabel = ch.Label
		m.loaderHint = ""
		m.nameStepSetFocus(focusWizardName)
		m.step = StepEnterName
		// Suggest "<version> <Loader>", but never overwrite a name the user typed.
		suggested := fmt.Sprintf("%s %s", m.selectedVersion, ch.Label)
		if v := strings.TrimSpace(m.nameInput.Value()); v == "" || v == m.autoName {
			m.nameInput.SetValue(suggested)
		}
		m.autoName = suggested
		m.nameInput.Focus()
	}
	return m, nil
}

func (m *WizardModel) nameStepFocusOrder() []nameFormFocus {
	if m.selectedLoader == "fabric" {
		return []nameFormFocus{focusWizardName, focusWizardStarterCheckbox}
	}
	return []nameFormFocus{focusWizardName}
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

// handleNameStepEnter creates the instance — Enter does that from any field
// (space toggles the starter-mods row).
// nameStepMoveFocus moves between the name step's rows (↑/↓), stopping at the
// first and last instead of wrapping (tab still wraps).
func (m *WizardModel) nameStepMoveFocus(dir int) {
	order := m.nameStepFocusOrder()
	i := 0
	for j, f := range order {
		if f == m.nameFormFocus {
			i = j
		}
	}
	m.nameStepSetFocus(order[min(len(order)-1, max(0, i+dir))])
}

func (m *WizardModel) handleNameStepEnter() (*WizardModel, tea.Cmd) {
	return m.submitNameStep()
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
	next := min(len(selectable)-1, max(0, cur+delta))
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

// stepBreadcrumb renders the step trail, e.g. "✓ Version  ›  Loader  ›  Name":
// done steps get a green check, the current one is bright and bold, upcoming
// ones are dim.
func (m *WizardModel) stepBreadcrumb() string {
	steps := []string{"Version", "Loader", "Name"}
	sep := lipgloss.NewStyle().Foreground(Active.TextFaint).Render("  ›  ")
	parts := make([]string, 0, len(steps))
	for i, s := range steps {
		switch {
		case i == int(m.step):
			parts = append(parts, lipgloss.NewStyle().Bold(true).Foreground(Active.TextStrong).Render(s))
		case i < int(m.step):
			parts = append(parts, lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDone)+" "+
				lipgloss.NewStyle().Foreground(Active.TextDim).Render(s))
		default:
			parts = append(parts, lipgloss.NewStyle().Foreground(Active.TextMuted).Render(s))
		}
	}
	return strings.Join(parts, sep)
}

// View implements tea.Model
func (m *WizardModel) View() string {
	base := m.baseView()
	switch {
	case m.help:
		return Overlay(base, HelpCard("New instance", m.helpGroups(), m.width, m.height), m.width, m.height)
	case m.palette.Active:
		return Overlay(base, m.palette.View(m.width, m.height), m.width, m.height)
	}
	return base
}

// baseView renders the screen itself (overlays composite over it in View).
func (m *WizardModel) baseView() string {
	header := AppHeader(m.width, "", "New instance")
	if m.err != nil {
		w := m.panelWidth()
		headline := lipgloss.NewStyle().Bold(true).Foreground(Active.Error).
			Render(GlyphFail + " Couldn't load the Minecraft version list")
		detail := lipgloss.NewStyle().Foreground(Active.TextDim).Width(w).
			Render(fmt.Sprintf("Check your connection and try again. (%v)", m.err))
		hints := KeyHints(w, KeyHint{"r", "retry"}, KeyHint{"esc", "back"})
		return lipgloss.JoinVertical(lipgloss.Left, header, "", headline, detail, "", hints)
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
	return lipgloss.JoinVertical(lipgloss.Left, header, "", m.stepBreadcrumb(), "", content)
}

func (m *WizardModel) viewVersionStep() string {
	w := m.panelWidth()
	if m.loading {
		return lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Loading Minecraft versions…")
	}
	snaps := "releases only"
	if m.showSnaps {
		snaps = "incl. snapshots"
	}
	right := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(snaps)
	pane := ZoneR("Minecraft version", right, m.versionList.View(), w, true)

	hints := []KeyHint{{"↵", "next"}, {"/", "filter"}}
	if m.showSnaps {
		hints = append(hints, KeyHint{"space", "hide snapshots"})
	} else {
		hints = append(hints, KeyHint{"space", "show snapshots"})
	}
	hints = append(hints, KeyHint{"esc", "cancel"}, KeyHint{"?", "help"})
	if m.versionList.FilterState() == list.Filtering {
		hints = []KeyHint{{"↵", "apply filter"}, {"esc", "clear filter"}}
	}
	return lipgloss.JoinVertical(lipgloss.Left, pane, "", KeyHints(m.width, hints...))
}

func (m *WizardModel) viewLoaderStep() string {
	w := m.panelWidth()
	var rows []string
	for i, ch := range m.loaderChoices {
		selected := i == m.loaderIndex
		st := lipgloss.NewStyle().Foreground(Active.Text)
		switch {
		case selected:
			st = st.Bold(true).Foreground(Active.TextStrong)
		case ch.ComingSoon:
			st = st.Foreground(Active.TextFaint)
		}
		rows = append(rows, rowMarker(selected)+st.Render(ch.Label))
	}
	right := lipgloss.NewStyle().Foreground(Active.TextMuted).Render("Minecraft " + m.selectedVersion)
	pane := ZoneR("Mod loader", right, "\n"+lipgloss.JoinVertical(lipgloss.Left, rows...), w, true)

	parts := []string{pane, ""}
	if m.loaderHint != "" {
		parts = append(parts, lipgloss.NewStyle().Foreground(Active.Warning).Render(GlyphWarn+" "+m.loaderHint), "")
	}
	parts = append(parts, KeyHints(m.width, KeyHint{"↵", "next"}, KeyHint{"↑↓", "choose"}, KeyHint{"esc", "back"}, KeyHint{"?", "help"}))
	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}

func (m *WizardModel) viewNameStep() string {
	w := m.panelWidth()
	rows := []string{"", formRow("Name", m.nameInput.View(), "Shown in the instance list. You can rename it later.", m.nameFormFocus == focusWizardName, w)}
	if m.selectedLoader == "fabric" {
		value := formToggle(m.installStarterMods)
		rows = append(rows, formRow("Starter mods", value,
			"Fabric API, Mod Menu, Sodium and Lithium, installed on first launch. Space to toggle.",
			m.nameFormFocus == focusWizardStarterCheckbox, w))
	}
	if m.nameErr != "" {
		rows = append(rows, "", lipgloss.NewStyle().Foreground(Active.Error).Render(GlyphFail+" "+m.nameErr))
	}
	right := lipgloss.NewStyle().Foreground(Active.TextMuted).Render("Minecraft " + m.selectedVersion + " · " + m.selectedLoaderLabel)
	pane := ZoneR("Name your instance", right, lipgloss.JoinVertical(lipgloss.Left, rows...), w, true)

	hints := []KeyHint{{"↵", "create"}}
	if m.selectedLoader == "fabric" {
		hints = append(hints, KeyHint{"↑↓", "move"})
		if m.nameFormFocus == focusWizardStarterCheckbox {
			hints = append(hints, KeyHint{"space", "toggle"}, KeyHint{"?", "help"})
		}
	}
	hints = append(hints, KeyHint{"esc", "back"})
	return lipgloss.JoinVertical(lipgloss.Left, pane, "", KeyHints(m.width, hints...))
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
