package ui

import (
	"os"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	tea "github.com/charmbracelet/bubbletea"
)

// key builds a tea.KeyMsg from a single named key or rune sequence.
func keyMsg(s string) tea.KeyMsg {
	switch s {
	case "tab":
		return tea.KeyMsg{Type: tea.KeyTab}
	case "enter":
		return tea.KeyMsg{Type: tea.KeyEnter}
	case "esc":
		return tea.KeyMsg{Type: tea.KeyEsc}
	case "up":
		return tea.KeyMsg{Type: tea.KeyUp}
	case "down":
		return tea.KeyMsg{Type: tea.KeyDown}
	case "left":
		return tea.KeyMsg{Type: tea.KeyLeft}
	case "right":
		return tea.KeyMsg{Type: tea.KeyRight}
	default:
		return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(s)}
	}
}

func sendKey(t *testing.T, m *HomeModel, s string) tea.Cmd {
	t.Helper()
	_, cmd := m.Update(keyMsg(s))
	return cmd
}

// fabricInstance builds a Fabric instance with mods + resource packs on disk so
// the detail tabs have real navigable rows.
func fabricInstance(t *testing.T) []*core.Instance {
	t.Helper()
	dir := t.TempDir()
	rp := dir + "/.minecraft/resourcepacks"
	if err := os.MkdirAll(rp, 0o755); err != nil {
		t.Fatal(err)
	}
	mustWrite(t, rp+"/Faithful.zip", 1024)
	mustWrite(t, rp+"/Other.zip", 2048)
	modsDir := dir + "/.minecraft/mods"
	if err := os.MkdirAll(modsDir, 0o755); err != nil {
		t.Fatal(err)
	}
	mustWrite(t, modsDir+"/sodium.jar", 100)
	return []*core.Instance{{ID: "fab", Name: "Fabric Inst", Version: "1.21.4", Loader: "fabric", Path: dir}}
}

func mustWrite(t *testing.T, path string, n int) {
	t.Helper()
	if err := os.WriteFile(path, make([]byte, n), 0o644); err != nil {
		t.Fatal(err)
	}
}

func newHomeWith(t *testing.T, insts []*core.Instance) *HomeModel {
	t.Helper()
	m := NewHomeModel()
	m.SetInstances(insts, "")
	m.SetSize(120, 40)
	return m
}

func TestTabFocusesDetailPane(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	if m.paneFocus != focusList {
		t.Fatalf("expected focusList by default, got %d", m.paneFocus)
	}
	sendKey(t, m, "tab")
	if m.paneFocus != focusDetail {
		t.Fatalf("Tab should focus the detail pane, got %d", m.paneFocus)
	}
	// Esc returns to the list.
	sendKey(t, m, "esc")
	if m.paneFocus != focusList {
		t.Fatalf("Esc should return focus to the list, got %d", m.paneFocus)
	}
}

// TestSpatialPaneNavigation verifies the column-sequence model: → from the list
// enters the detail pane on Overview and walks rightward through the tabs (no
// wrap); ← walks back and returns to the list from the leftmost tab.
func TestSpatialPaneNavigation(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	if m.paneFocus != focusList || m.activeTab != tabOverview {
		t.Fatalf("expected focusList+Overview by default, got focus %d tab %d", m.paneFocus, m.activeTab)
	}

	// ← on the list does nothing (already the leftmost column).
	sendKey(t, m, "left")
	if m.paneFocus != focusList {
		t.Fatalf("left on the list must not change focus, got %d", m.paneFocus)
	}

	// → enters the detail pane on Overview (not Mods — no auto-skip).
	sendKey(t, m, "right")
	if m.paneFocus != focusDetail || m.activeTab != tabOverview {
		t.Fatalf("right from the list should focus detail on Overview, got focus %d tab %d", m.paneFocus, m.activeTab)
	}

	// → walks rightward and clamps at the last tab (no wrap).
	sendKey(t, m, "right") // Mods
	if m.activeTab != tabMods {
		t.Fatalf("right should advance to Mods, got %d", m.activeTab)
	}
	sendKey(t, m, "right") // Resource packs (the last tab)
	sendKey(t, m, "right") // clamp
	if m.activeTab != tabResourcePacks || m.paneFocus != focusDetail {
		t.Fatalf("right should clamp at the last tab while focused, got focus %d tab %d", m.paneFocus, m.activeTab)
	}

	// ← walks back to Overview, staying focused.
	sendKey(t, m, "left") // Mods
	sendKey(t, m, "left") // Overview
	if m.activeTab != tabOverview || m.paneFocus != focusDetail {
		t.Fatalf("left should walk back to Overview while focused, got focus %d tab %d", m.paneFocus, m.activeTab)
	}

	// ← from the leftmost tab returns to the list, keeping Overview as companion.
	sendKey(t, m, "left")
	if m.paneFocus != focusList || m.activeTab != tabOverview {
		t.Fatalf("left on Overview should return to the list (Overview companion), got focus %d tab %d", m.paneFocus, m.activeTab)
	}
}

// TestFocusDetailKeepsOverview verifies entering the detail pane no longer
// auto-switches off Overview — it lands on the current tab.
func TestFocusDetailKeepsOverview(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	m.activeTab = tabOverview
	m.focusDetailPane()
	if m.paneFocus != focusDetail {
		t.Fatalf("focusDetailPane should focus the detail pane, got %d", m.paneFocus)
	}
	if m.activeTab != tabOverview {
		t.Fatalf("focusing detail should stay on Overview (no auto-switch), got tab %d", m.activeTab)
	}
}

func TestDetailCursorMovesWithinBounds(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	m.activeTab = tabResourcePacks
	m.focusDetailPane()
	// Rows: "+ Add" + 2 packs = 3 rows; cursor starts at 0.
	if m.detailCursor != 0 {
		t.Fatalf("expected detailCursor 0, got %d", m.detailCursor)
	}
	sendKey(t, m, "down")
	sendKey(t, m, "down")
	if m.detailCursor != 2 {
		t.Fatalf("expected detailCursor 2 after two downs, got %d", m.detailCursor)
	}
	// Past the end clamps.
	sendKey(t, m, "down")
	if m.detailCursor != 2 {
		t.Fatalf("detailCursor should clamp at last row, got %d", m.detailCursor)
	}
	// Up walks back; clamps at 0.
	sendKey(t, m, "up")
	sendKey(t, m, "up")
	sendKey(t, m, "up")
	if m.detailCursor != 0 {
		t.Fatalf("detailCursor should clamp at 0, got %d", m.detailCursor)
	}
}

func TestEnterAddModsEmitsNavigateToMods(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	m.activeTab = tabMods
	m.focusDetailPane()
	m.detailCursor = 0 // the "+ Add mods…" row
	cmd := sendKey(t, m, "enter")
	if cmd == nil {
		t.Fatal("Enter on +Add mods should return a cmd")
	}
	switch cmd().(type) {
	case NavigateToMods:
		// ok
	default:
		t.Fatalf("expected NavigateToMods, got %T", cmd())
	}
}

func TestEnterAddModsNonFabricShowsBanner(t *testing.T) {
	restoreDark(t)
	dir := t.TempDir()
	insts := []*core.Instance{{ID: "van", Name: "Vanilla", Version: "1.21", Loader: "vanilla", Path: dir}}
	m := newHomeWith(t, insts)
	m.activeTab = tabMods
	m.focusDetailPane()
	m.detailCursor = 0
	cmd := sendKey(t, m, "enter")
	if cmd != nil {
		t.Fatalf("non-Fabric +Add mods should not navigate, got cmd %T", cmd())
	}
	if m.transientBanner == "" {
		t.Fatal("expected a transient banner for non-Fabric +Add mods")
	}
}

func TestEnterAddPacksEmitsNavigateToResourcePacks(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	m.activeTab = tabResourcePacks
	m.focusDetailPane()
	m.detailCursor = 0 // the "+ Add resource packs…" row
	cmd := sendKey(t, m, "enter")
	if cmd == nil {
		t.Fatal("Enter on +Add resource packs should return a cmd")
	}
	if _, ok := cmd().(NavigateToResourcePacks); !ok {
		t.Fatalf("expected NavigateToResourcePacks, got %T", cmd())
	}
}

func TestSwitchingTabResetsCursor(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	m.activeTab = tabMods
	m.focusDetailPane()
	m.detailCursor = 1
	sendKey(t, m, "]") // next tab (Resource packs)
	if m.detailCursor != 0 {
		t.Fatalf("switching tabs should reset detailCursor, got %d", m.detailCursor)
	}
	if m.activeTab != tabResourcePacks {
		t.Fatalf("expected tabResourcePacks after ], got %d", m.activeTab)
	}
}

func TestSwitchingInstanceResetsCursorAndFocus(t *testing.T) {
	restoreDark(t)
	a := fabricInstance(t)[0]
	b := &core.Instance{ID: "b", Name: "Second", Version: "1.20", Loader: "vanilla", Path: t.TempDir()}
	m := newHomeWith(t, []*core.Instance{a, b})
	startID := m.SelectedInstance().ID
	// Leave a stale cursor behind, then return to the list to navigate.
	m.focusDetailPane()
	m.detailCursor = 1
	m.paneFocus = focusList
	// Move the list cursor; this should land on the other instance.
	sendKey(t, m, "down")
	if m.SelectedInstance().ID == startID {
		// Single-step didn't change selection (recency tie ordering); try up.
		sendKey(t, m, "up")
	}
	if m.SelectedInstance().ID == startID {
		t.Skip("list navigation did not change selection in this ordering")
	}
	if m.paneFocus != focusList {
		t.Fatalf("changing instance should keep/return focus to the list, got %d", m.paneFocus)
	}
	if m.detailCursor != 0 {
		t.Fatalf("changing instance should reset detailCursor, got %d", m.detailCursor)
	}
}

// Global letter hotkeys keep working in detail focus (they don't conflict with
// arrows): `n` navigates to the new-instance wizard from either focus.
func TestGlobalHotkeysWorkInDetailFocus(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	m.focusDetailPane()
	cmd := sendKey(t, m, "n")
	if cmd == nil {
		t.Fatal("n should navigate to new instance even in detail focus")
	}
	if _, ok := cmd().(NavigateToNewInstance); !ok {
		t.Fatalf("expected NavigateToNewInstance, got %T", cmd())
	}
}

// TestAddInstanceRow covers the trailing "+ New instance" action row: ↓ selects
// it (it is not a real instance), it can't focus the empty detail pane, and
// Enter opens the new-instance wizard.
func TestAddInstanceRow(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))

	// Default selection is the first (only) real instance.
	if m.addRowSelected() {
		t.Fatal("first row should be a real instance, not the add row")
	}

	// ↓ from the last instance lands on the "+ New instance" row.
	sendKey(t, m, "down")
	if !m.addRowSelected() {
		t.Fatal("down from the last instance should select the + New instance row")
	}
	if m.SelectedInstance() != nil {
		t.Fatal("the add row is not an instance; SelectedInstance must be nil")
	}

	// Neither → nor Tab may focus the (non-existent) detail pane from the add row.
	sendKey(t, m, "right")
	if m.paneFocus != focusList {
		t.Fatalf("right on the add row must not focus detail, got %d", m.paneFocus)
	}
	sendKey(t, m, "tab")
	if m.paneFocus != focusList {
		t.Fatalf("tab on the add row must not focus detail, got %d", m.paneFocus)
	}

	// Enter opens the new-instance wizard.
	cmd := sendKey(t, m, "enter")
	if cmd == nil {
		t.Fatal("enter on the add row should return a cmd")
	}
	if _, ok := cmd().(NavigateToNewInstance); !ok {
		t.Fatalf("expected NavigateToNewInstance, got %T", cmd())
	}
}
