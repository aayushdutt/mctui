package ui

import (
	"reflect"
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/lipgloss"
)

func TestEditInstanceViewNonEmptyWithinWidth(t *testing.T) {
	Apply("dark")
	inst := &core.Instance{
		ID: "survival", Name: "Survival World", Version: "1.21.4", Loader: "fabric",
		Path: "/tmp/instances/survival", JVMArgs: []string{"-Xmx4G", "-XX:+UseG1GC"},
	}
	m := NewEditInstanceModel(inst)
	const w = 80
	m.SetSize(w, 26)
	out := m.View()
	if strings.TrimSpace(out) == "" {
		t.Fatal("View() returned empty output")
	}
	for i, line := range strings.Split(out, "\n") {
		if lw := lipgloss.Width(line); lw > w {
			t.Errorf("line %d width %d exceeds %d: %q", i, lw, w, line)
		}
	}
	if !strings.Contains(out, "Survival World") {
		t.Error("View() should title the screen with the instance name")
	}
}

func TestSplitXmx(t *testing.T) {
	cases := []struct {
		name     string
		args     []string
		wantXmx  string
		wantRest []string
	}{
		{"none", []string{"-XX:+UseG1GC"}, "", []string{"-XX:+UseG1GC"}},
		{"only xmx", []string{"-Xmx4G"}, "4G", []string{}},
		{"mixed", []string{"-XX:+UseG1GC", "-Xmx2048M", "-Dfoo=bar"}, "2048M", []string{"-XX:+UseG1GC", "-Dfoo=bar"}},
		{"last wins", []string{"-Xmx1G", "-Xmx8G"}, "8G", []string{}},
		{"empty", nil, "", []string{}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			xmx, rest := splitXmx(c.args)
			if xmx != c.wantXmx {
				t.Errorf("xmx = %q, want %q", xmx, c.wantXmx)
			}
			if !reflect.DeepEqual(rest, c.wantRest) {
				t.Errorf("rest = %v, want %v", rest, c.wantRest)
			}
		})
	}
}

func TestFoldXmx(t *testing.T) {
	cases := []struct {
		name string
		mem  string
		rest []string
		want []string
	}{
		{"adds xmx", "4G", []string{"-XX:+UseG1GC"}, []string{"-Xmx4G", "-XX:+UseG1GC"}},
		{"empty mem drops xmx", "", []string{"-Xmx2G", "-Dfoo=bar"}, []string{"-Dfoo=bar"}},
		{"dedups stray xmx", "8G", []string{"-Xmx1G", "-Dfoo=bar"}, []string{"-Xmx8G", "-Dfoo=bar"}},
		{"all empty", "", nil, nil},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := foldXmx(c.mem, c.rest)
			if !reflect.DeepEqual(got, c.want) {
				t.Errorf("foldXmx(%q, %v) = %v, want %v", c.mem, c.rest, got, c.want)
			}
		})
	}
}

// TestEditInstanceRoundTrip verifies the -Xmx surfaced into the Memory field and
// the remaining args round-trip back through a save into JVMArgs.
func TestEditInstanceRoundTrip(t *testing.T) {
	Apply("dark")
	inst := &core.Instance{
		ID: "x", Name: "X", Path: "/tmp/x",
		JVMArgs: []string{"-Xmx4G", "-XX:+UseG1GC"},
	}
	m := NewEditInstanceModel(inst)
	if got := m.memory.Value(); got != "4G" {
		t.Errorf("memory prefilled = %q, want 4G", got)
	}
	if got := m.jvmArgs.Value(); got != "-XX:+UseG1GC" {
		t.Errorf("jvmArgs prefilled = %q, want -XX:+UseG1GC", got)
	}
	m.memory.SetValue("8G")
	_, _ = m.submit()
	want := []string{"-Xmx8G", "-XX:+UseG1GC"}
	if !reflect.DeepEqual(inst.JVMArgs, want) {
		t.Errorf("after save JVMArgs = %v, want %v", inst.JVMArgs, want)
	}
	if inst.ID != "x" || inst.Path != "/tmp/x" {
		t.Error("save must not mutate ID/Path")
	}
}
