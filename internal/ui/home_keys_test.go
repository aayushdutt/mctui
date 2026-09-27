package ui

import (
	"fmt"
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func manyInstances(n int) []*core.Instance {
	out := make([]*core.Instance, n)
	for i := range out {
		out[i] = &core.Instance{ID: fmt.Sprintf("id%02d", i), Name: fmt.Sprintf("Inst %02d", i), Version: "1.21.4"}
	}
	return out
}

// Typing into the filter must capture q (the app's global quit checks this).
func TestHome_FilterCapturesQ(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 40)
	m.SetInstances(manyInstances(3), "")

	m.Update(keyMsg("/"))
	if m.list.FilterState() != list.Filtering {
		t.Fatal("setup: / should start filtering")
	}
	if !m.CapturesInput() {
		t.Error("home must report it captures input while filtering")
	}
	typeRunes(m, "q")
	if got := m.list.FilterValue(); got != "q" {
		t.Errorf("filter value = %q, want %q", got, "q")
	}
}

// Home's letter keys and pane/tab steps must not also page the list (the list's
// defaults bound h/l/←/→/f/d/b/u to paging, so e.g. f opened the folder AND
// jumped a page, and the next f/Enter acted on another instance).
func TestHome_HotkeysDontPageTheList(t *testing.T) {
	restoreDark(t)
	opened := 0
	orig := openInstanceFolder
	openInstanceFolder = func(string) { opened++ }
	t.Cleanup(func() { openInstanceFolder = orig })

	for _, tc := range []struct {
		name  string
		width int
		onAdd bool // park on the "+ New instance" row (d then has no instance to arm)
		keys  []string
	}{
		{"compact steps", 70, false, []string{"l", "right", "h", "left"}},
		{"folder on an instance", 120, false, []string{"f"}},
		{"stray letters", 120, true, []string{"d", "b", "u"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			m := NewHomeModel()
			m.SetSize(tc.width, 24)
			m.SetInstances(manyInstances(20), "")
			if tc.onAdd {
				m.list.Select(len(m.list.Items()) - 1)
			}
			before := m.list.Index()
			for _, k := range tc.keys {
				m.Update(keyMsg(k))
				if got := m.list.Index(); got != before {
					t.Errorf("%q moved the cursor %d -> %d", k, before, got)
				}
			}
		})
	}
	if opened != 1 {
		t.Errorf("f should open the folder once, opened %d times", opened)
	}
}

// With the detail pane focused, list keys don't move the hidden list, and /
// returns to the list to search.
func TestHome_DetailFocusOwnsListKeys(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 30)
	m.SetInstances(manyInstances(20), "")
	m.Update(keyMsg("right")) // focus the detail pane
	before := m.list.Index()
	for _, k := range []tea.KeyMsg{{Type: tea.KeyPgDown}, {Type: tea.KeyEnd}, keyMsg("G")} {
		m.Update(k)
		if m.list.Index() != before {
			t.Errorf("%q moved the list from the detail pane", k.String())
		}
	}
	m.Update(keyMsg("/"))
	if m.paneFocus != focusList || m.list.FilterState() != list.Filtering {
		t.Errorf("/ from the detail pane should focus the list and filter (focus=%v filter=%v)", m.paneFocus, m.list.FilterState())
	}
}

// No instances: the list isn't drawn, so / must not start an invisible filter
// (after which a/s/q would type into it).
func TestHome_SlashIgnoredWithNoInstances(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 30)
	m.SetInstances(nil, "")
	m.Update(keyMsg("/"))
	if m.list.FilterState() != list.Unfiltered {
		t.Error("/ started a filter on the empty screen")
	}
}

// Shrinking to one column returns focus to the list (the detail pane is gone).
func TestHome_CompactResizeReturnsFocusToList(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 30)
	m.SetInstances(manyInstances(3), "")
	m.Update(keyMsg("right"))
	m.SetSize(60, 30)
	if m.paneFocus != focusList {
		t.Error("focus stayed on the hidden detail pane")
	}
}

// While typing a filter the footer offers only what works.
func TestHome_FilteringFooter(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 30)
	m.SetInstances(manyInstances(3), "")
	m.Update(keyMsg("/"))
	f := ansi.Strip(m.buildFooter())
	if !strings.Contains(f, "apply filter") || strings.Contains(f, "offline") {
		t.Errorf("filtering footer = %q", f)
	}
}
