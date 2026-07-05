package ui

import (
	"errors"
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
)

// Advancement-style copy (spec M3.2): done reads as an unlocked advancement,
// running keeps the technical name in parens, errors stay plain (no joke).
func TestLaunch_StepRowCopy(t *testing.T) {
	if got := stepRowText(stepInfo{name: "Checking Java", status: "done"}); got != "Advancement made: Java acquired" {
		t.Fatalf("done copy = %q", got)
	}
	if got := stepRowText(stepInfo{name: "Downloading assets", status: "running"}); got != "Punching trees… (downloading assets)" {
		t.Fatalf("running copy = %q", got)
	}
	// Errors never get flavor text.
	if got := stepRowText(stepInfo{name: "Downloading assets", status: "error"}); got != "Downloading assets" {
		t.Fatalf("error copy must stay plain, got %q", got)
	}
	// Unmapped step (e.g. starter mods) falls back to its plain name while running.
	if got := stepRowText(stepInfo{name: "Installing starter mods", status: "running"}); got != "Installing starter mods" {
		t.Fatalf("unmapped running copy = %q", got)
	}
}

// The XP bar fills proportionally, clamps, and always shows a percentage.
func TestLaunch_XPBar(t *testing.T) {
	restoreDark(t)
	empty := xpBar(0, 40)
	full := xpBar(1, 40)
	if strings.Contains(empty, "▰") {
		t.Error("0% bar should have no filled segments")
	}
	if strings.Contains(full, "▱") {
		t.Error("100% bar should have no empty segments")
	}
	if !strings.Contains(empty, "0%") || !strings.Contains(full, "100%") {
		t.Error("xp bar should render the percentage")
	}
	// Over/under-range percentages clamp rather than panic or overflow.
	_ = xpBar(-0.5, 40)
	_ = xpBar(1.5, 40)
}

// Leaving the launch screen while a launch is still in progress must cancel it
// (CancelLaunch tears down the launch context + status channel) rather than just
// navigating home and leaking the launch goroutine. Regression test for the
// Phase-1 review finding: the palette "Back to home" verb bypassed cancellation.
func TestLaunch_LeaveWhileInProgressCancels(t *testing.T) {
	restoreDark(t)
	m := NewLaunchModel(&core.Instance{Name: "W", Version: "1.21", Loader: "fabric"}, config.DefaultConfig())

	// In progress: leaving must emit CancelLaunch.
	cmd := m.leaveLaunch()
	if cmd == nil {
		t.Fatal("leaveLaunch should return a command")
	}
	if _, ok := cmd().(CancelLaunch); !ok {
		t.Fatalf("in-progress leave should CancelLaunch, got %T", cmd())
	}
	// The palette verb uses the same path.
	items := m.launchPaletteItems()
	if items[0].Label != "Cancel launch" {
		t.Fatalf("in-progress leave verb should read 'Cancel launch', got %q", items[0].Label)
	}
	if _, ok := items[0].Run()().(CancelLaunch); !ok {
		t.Fatal("in-progress palette leave verb must CancelLaunch, not NavigateToHome")
	}

	// Completed: leaving just navigates home.
	m.done = true
	m.err = errors.New("boom")
	if _, ok := m.leaveLaunch()().(NavigateToHome); !ok {
		t.Fatal("a completed launch should leave via NavigateToHome")
	}
	if items := m.launchPaletteItems(); items[0].Label != "Back to home" {
		t.Fatalf("completed leave verb should read 'Back to home', got %q", items[0].Label)
	}
}
