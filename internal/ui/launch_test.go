package ui

import (
	"errors"
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/launch"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// Steps show their real names; only the final step reads as what the user is
// waiting for.
func TestLaunch_StepLabels(t *testing.T) {
	if got := stepLabel(stepInfo{name: "Downloading assets", status: "running"}); got != "Downloading assets" {
		t.Errorf("running label = %q", got)
	}
	if got := stepLabel(stepInfo{name: "Launching", status: "pending"}); got != "Starting Minecraft" {
		t.Errorf("final step label = %q", got)
	}
}

// The progress bar fills proportionally, clamps, and shows the percentage.
func TestLaunch_ProgressBar(t *testing.T) {
	restoreDark(t)
	empty := ansi.Strip(progressBar(0, 40))
	full := ansi.Strip(progressBar(1, 40))
	if strings.Contains(empty, "━") || !strings.Contains(empty, "0%") {
		t.Errorf("0%% bar = %q", empty)
	}
	if strings.Contains(full, "─") || !strings.Contains(full, "100%") {
		t.Errorf("100%% bar = %q", full)
	}
	for _, p := range []float64{-0.5, 1.5} {
		if w := lipgloss.Width(progressBar(p, 40)); w != 40 {
			t.Errorf("bar at %v is %d wide, want 40", p, w)
		}
	}
}

// Overall progress only moves forward (the launcher mixes a download step's own
// fraction with 0-progress info and log statuses) and reads 100% once playing.
func TestLaunch_ProgressIsMonotonicAndFullWhilePlaying(t *testing.T) {
	restoreDark(t)
	m := NewLaunchModel(&core.Instance{Name: "W", Version: "1.21"}, config.DefaultConfig())
	steps := []launch.Status{
		{Step: "Checking Java", Progress: 0},
		{Step: "Downloading libraries", Progress: 0.5},
		{Step: "Downloading libraries", Progress: 0.2}, // stale/lower fraction
		{Step: "Playing", LogLine: &launch.LogLine{Text: "x"}},
	}
	last := -1.0
	for _, st := range steps[:3] {
		m.Update(LaunchStatusUpdate{Status: st})
		if m.percent < last {
			t.Fatalf("progress went backwards: %.2f -> %.2f", last, m.percent)
		}
		last = m.percent
	}
	if m.percent <= 0.2 || m.percent >= 0.4 {
		t.Errorf("halfway through step 2 of 5, progress = %.2f, want ~0.3", m.percent)
	}
	m.Update(LaunchStatusUpdate{Status: launch.Status{Step: "Playing"}})
	if m.percent != 1 {
		t.Errorf("playing progress = %.2f, want 1", m.percent)
	}
}

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
	m0 := playingLaunchModel(t)
	_, cmd0 := m0.Update(keyMsg("esc"))
	if _, ok := cmdMsg(cmd0).(LeaveLaunchScreen); !ok {
		t.Errorf("esc while playing: got %T, want LeaveLaunchScreen", cmdMsg(cmd0))
	}
	// q only quits from home; here it does nothing (it never stops the game).
	if _, cmd := playingLaunchModel(t).Update(keyMsg("q")); cmdMsg(cmd) != nil {
		t.Errorf("q on the launch screen should do nothing, got %T", cmdMsg(cmd))
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
