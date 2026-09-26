package ui

import (
	"fmt"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/bubbles/list"
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
