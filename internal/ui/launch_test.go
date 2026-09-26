package ui

import (
	"errors"
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/launch"
	tea "github.com/charmbracelet/bubbletea"
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

func playingLaunchModel(t *testing.T) *LaunchModel {
	t.Helper()
	restoreDark(t)
	m := NewLaunchModel(&core.Instance{Name: "W", Version: "1.21", Loader: "fabric"}, config.DefaultConfig())
	m.SetSize(100, 30)
	m.Update(LaunchStatusUpdate{Status: launch.Status{Step: "Playing", Message: "Game running..."}})
	if !m.playing() {
		t.Fatal("setup: expected the playing state")
	}
	return m
}

func cmdMsg(cmd tea.Cmd) tea.Msg {
	if cmd == nil {
		return nil
	}
	return cmd()
}

// esc/q while the game runs used to CancelLaunch — which SIGKILLed Minecraft.
// Leaving now returns home and the game keeps running.
func TestLaunch_LeavingWhilePlayingKeepsTheGame(t *testing.T) {
	for _, k := range []string{"esc", "q"} {
		m := playingLaunchModel(t)
		_, cmd := m.Update(keyMsg(k))
		if _, ok := cmdMsg(cmd).(LeaveLaunchScreen); !ok {
			t.Errorf("%s while playing: got %T, want LeaveLaunchScreen", k, cmdMsg(cmd))
		}
	}
	m := playingLaunchModel(t)
	items := m.launchPaletteItems()
	if _, ok := items[0].Run()().(LeaveLaunchScreen); !ok {
		t.Errorf("palette %q should leave without stopping the game", items[0].Label)
	}
}

// Stopping the game is explicit and confirmed, with "Keep playing" focused.
func TestLaunch_StopGameNeedsConfirmation(t *testing.T) {
	m := playingLaunchModel(t)
	m.Update(keyMsg("s"))
	if !m.confirmStop {
		t.Fatal("s while playing should ask to stop the game")
	}
	if !strings.Contains(m.View(), "Stop Minecraft?") {
		t.Error("the stop confirmation should render")
	}
	if _, cmd := m.Update(keyMsg("enter")); cmd != nil || m.confirmStop {
		t.Error("enter on the default (Keep playing) must close the dialog without stopping")
	}

	m.Update(keyMsg("s"))
	m.Update(keyMsg("left"))
	_, cmd := m.Update(keyMsg("enter"))
	if _, ok := cmdMsg(cmd).(CancelLaunch); !ok {
		t.Errorf("confirming Stop game should CancelLaunch, got %T", cmdMsg(cmd))
	}

	m = playingLaunchModel(t)
	m.Update(keyMsg("s"))
	if _, cmd := m.Update(keyMsg("esc")); cmd != nil || m.confirmStop {
		t.Error("esc should dismiss the stop dialog without leaving or stopping")
	}
}

// If the game exits while "Stop Minecraft?" is open, the dialog goes away (it
// would hide the result, and confirming would discard it).
func TestLaunch_StopDialogClosesWhenGameExits(t *testing.T) {
	m := playingLaunchModel(t)
	m.Update(keyMsg("s"))
	m.Update(LaunchComplete{Error: errors.New("crashed")})
	if m.confirmStop {
		t.Error("the stop dialog should close when the launch completes")
	}
	if m.askStopGame(); m.confirmStop {
		t.Error("stop can't be asked once the game has exited")
	}
}
