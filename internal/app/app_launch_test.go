package app

import (
	"errors"
	"os"
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/launch"
	"github.com/aayushdutt/mctui/internal/ui"
	tea "github.com/charmbracelet/bubbletea"
)

// These drive Update directly and never run the returned commands, so no launch
// goroutine actually starts; stopLaunch in cleanup releases the contexts.

func newLaunchTestModel(t *testing.T) (*Model, *core.Instance) {
	t.Helper()
	m := newTestModel(t)
	t.Cleanup(m.stopLaunch)
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	return m, &core.Instance{ID: "inst", Name: "Inst", Version: "1.21.4", Path: t.TempDir()}
}

// A second Enter while the online session check runs used to start a second
// check and then a second launch (two JVMs, first one uncancellable).
func TestLaunch_OnlyOneSessionGateAtATime(t *testing.T) {
	m, inst := newLaunchTestModel(t)

	if _, cmd := m.Update(ui.NavigateToLaunch{Instance: inst}); cmd == nil {
		t.Fatal("first online launch should start the session gate")
	}
	if _, cmd := m.Update(ui.NavigateToLaunch{Instance: inst}); cmd != nil {
		t.Error("a second launch while the gate is in flight must be ignored")
	}
	if _, cmd := m.Update(ui.NavigateToLaunch{Instance: inst, Offline: true}); cmd != nil {
		t.Error("an offline launch while the gate is in flight must be ignored")
	}

	m.Update(ui.ProceedWithLaunch{Instance: inst})
	if m.state != StateLaunch || !m.launchLive {
		t.Fatalf("gate pass should open the launch (state=%v live=%v)", m.state, m.launchLive)
	}
	if _, cmd := m.Update(ui.NavigateToLaunch{Instance: inst, Offline: true}); cmd != nil {
		t.Error("a launch while another is live must be ignored")
	}
}

// After a cancel, the old launch's in-flight messages must not touch the next
// launch (they used to nil its cancel func and channel) or navigate anywhere.
func TestLaunch_StaleMessagesAreDropped(t *testing.T) {
	m, inst := newLaunchTestModel(t)

	m.Update(ui.NavigateToLaunch{Instance: inst, Offline: true})
	first := m.launchID
	m.Update(ui.CancelLaunch{})
	if m.launchLive || m.launch != nil || m.state != StateHome {
		t.Fatalf("cancel should tear down the launch (live=%v launch=%v state=%v)", m.launchLive, m.launch != nil, m.state)
	}
	if _, cmd := m.Update(ui.LaunchComplete{ID: first}); cmd != nil {
		t.Error("a stale LaunchComplete after cancel must be ignored")
	}

	m.Update(ui.NavigateToLaunch{Instance: inst, Offline: true})
	second := m.launchID
	if second == first {
		t.Fatal("each launch needs a new ID")
	}
	if _, cmd := m.Update(ui.LaunchStatusUpdate{ID: first}); cmd != nil {
		t.Error("a stale status must not resubscribe")
	}
	m.Update(ui.LaunchComplete{ID: first})
	if !m.launchLive || m.launchCtxCancel == nil || m.launchStatusChan == nil {
		t.Error("a stale LaunchComplete must not tear down the current launch")
	}

	m.Update(ui.LaunchComplete{ID: second, Error: errors.New("boom")})
	if m.launchLive {
		t.Error("the current launch's LaunchComplete should end it")
	}
}

// Offline retry reused the failed screen (done/err still set): it kept saying
// "Failed", esc skipped the cancel, and every r/o press started another launch.
func TestLaunch_RetryStartsFreshAndOnlyOnce(t *testing.T) {
	m, inst := newLaunchTestModel(t)

	m.Update(ui.NavigateToLaunch{Instance: inst, Offline: true})
	m.Update(ui.LaunchComplete{ID: m.launchID, Error: errors.New("boom")})
	failed := m.launch

	m.Update(ui.RetryLaunch{Offline: true})
	if m.launch == failed {
		t.Fatal("retry should build a fresh launch screen")
	}
	if !m.launchLive {
		t.Fatal("retry should start a launch")
	}
	retryID := m.launchID
	if _, cmd := m.Update(ui.RetryLaunch{Offline: true}); cmd != nil || m.launchID != retryID {
		t.Error("a second retry while the first runs must be ignored")
	}

	// esc on the fresh screen cancels (it isn't "done").
	_, cmd := m.launch.Update(tea.KeyMsg{Type: tea.KeyEsc})
	if cmd == nil {
		t.Fatal("esc should produce a command")
	}
	if _, ok := cmd().(ui.CancelLaunch); !ok {
		t.Error("esc during the retried launch should cancel it")
	}
}

// Runs the real launch command: version resolution fails against the local test
// manifest (no 1.21.4 detail URL), and the result must come back tagged with
// the launch's ID so Update applies it.
func TestLaunch_ResultCarriesLaunchID(t *testing.T) {
	m, inst := newLaunchTestModel(t)

	cmd := m.beginLaunch(inst, true)
	msg, ok := cmd().(ui.LaunchComplete)
	if !ok {
		t.Fatalf("expected LaunchComplete, got %T", msg)
	}
	if msg.ID != m.launchID || msg.Error == nil {
		t.Errorf("LaunchComplete{ID:%d Error:%v}, want ID %d with an error", msg.ID, msg.Error, m.launchID)
	}
}

// Leaving the launch screen while the game runs keeps it running: home shows
// it, Enter on it reopens its screen, other launches wait, it can't be deleted,
// and its exit lands on home without yanking the user around.
func TestLaunch_DetachWhilePlaying(t *testing.T) {
	m, inst := newLaunchTestModel(t)
	if err := m.instances.Create(inst); err != nil {
		t.Fatal(err)
	}
	other := &core.Instance{ID: "other", Name: "Other", Version: "1.21.4"}

	m.Update(ui.NavigateToLaunch{Instance: inst, Offline: true})
	id := m.launchID
	m.Update(ui.LaunchStatusUpdate{ID: id, Status: launch.Status{Step: "Playing"}})

	m.Update(ui.LeaveLaunchScreen{})
	if m.state != StateHome || !m.launchLive {
		t.Fatalf("leaving should go home with the game still live (state=%v live=%v)", m.state, m.launchLive)
	}

	m.Update(ui.NavigateToLaunch{Instance: other, Offline: true})
	if m.state != StateHome || m.launchID != id {
		t.Error("launching another instance while one runs must not start it")
	}

	m.Update(ui.DeleteInstance{Instance: inst})
	if _, err := os.Stat(inst.Path); err != nil {
		t.Errorf("the running instance must not be deleted: %v", err)
	}

	m.Update(ui.NavigateToLaunch{Instance: inst, Offline: true})
	if m.state != StateLaunch || m.launchID != id {
		t.Fatalf("Enter on the running instance should reopen its screen (state=%v id=%d want %d)", m.state, m.launchID, id)
	}

	m.Update(ui.LeaveLaunchScreen{})
	_, cmd := m.Update(ui.LaunchComplete{ID: id, Error: errors.New("exit status 1")})
	if m.state != StateHome || m.launchLive || m.launch != nil {
		t.Errorf("the game exiting while away should end the launch on home (state=%v live=%v)", m.state, m.launchLive)
	}
	if cmd == nil {
		t.Error("a crash while away should raise a banner and reload instances")
	}
}

// playingDetached starts an (unstarted) offline launch, marks it playing, and
// leaves its screen, as esc does while the game runs.
func playingDetached(t *testing.T, m *Model, inst *core.Instance) int {
	t.Helper()
	m.Update(ui.NavigateToLaunch{Instance: inst, Offline: true})
	id := m.launchID
	m.Update(ui.LaunchStatusUpdate{ID: id, Status: launch.Status{Step: "Playing"}})
	return id
}

// Stop game interrupts the game but keeps the launch (and the one-launch lock)
// until it has exited — a relaunch meanwhile would put two games on one dir —
// and the expected interrupt exit isn't reported as a crash.
func TestLaunch_StoppingKeepsLaunchUntilExit(t *testing.T) {
	m, inst := newLaunchTestModel(t)
	id := playingDetached(t, m, inst)

	m.Update(ui.CancelLaunch{})
	if !m.launchLive || !m.launchStopping || m.state != StateLaunch {
		t.Fatalf("stop should wait for the exit (live=%v stopping=%v state=%v)", m.launchLive, m.launchStopping, m.state)
	}
	m.Update(ui.LeaveLaunchScreen{})
	if _, cmd := m.Update(ui.NavigateToLaunch{Instance: &core.Instance{ID: "other", Name: "Other"}, Offline: true}); m.launchID != id {
		_ = cmd
		t.Fatal("no new launch while the stopped game is still exiting")
	}

	_, cmd := m.Update(ui.LaunchComplete{ID: id, Error: errors.New("Launching: game exited with error: signal: interrupt")})
	if m.launchLive || m.launchStopping {
		t.Error("the game's exit should end the launch")
	}
	if cmd == nil {
		t.Fatal("expected a reload after the game exits")
	}
}

// q on home with a game still running asks for a second q (the game would keep
// running unmanaged and its playtime go unrecorded).
func TestQuit_WhileGameRunsNeedsSecondQ(t *testing.T) {
	m, inst := newLaunchTestModel(t)
	playingDetached(t, m, inst)
	m.Update(ui.LeaveLaunchScreen{})

	// The first q only arms (returns the banner's 4s tick, so don't run it).
	m.Update(keyRunes("q"))
	if !m.quitArmed {
		t.Fatal("first q with a game running should ask, not quit")
	}
	m.Update(keyRunes("j")) // any other key disarms
	if m.quitArmed {
		t.Fatal("another key should disarm the pending quit")
	}
	m.Update(keyRunes("q"))
	_, cmd := m.Update(keyRunes("q"))
	if cmd == nil {
		t.Fatal("a second q in a row should quit")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Error("a second q in a row should quit")
	}
}

// The running instance's mods, resource packs, and settings can't be changed
// (the game holds its files and rewrites options.txt on exit).
func TestLaunch_RunningInstanceScreensBlocked(t *testing.T) {
	m, inst := newLaunchTestModel(t)
	playingDetached(t, m, inst)
	m.Update(ui.LeaveLaunchScreen{})

	for name, msg := range map[string]tea.Msg{
		"mods":           ui.NavigateToMods{Instance: inst},
		"resource packs": ui.NavigateToResourcePacks{Instance: inst},
		"edit":           ui.NavigateToEditInstance{Instance: inst},
	} {
		m.Update(msg)
		if m.state != StateHome {
			t.Errorf("%s: opened for the running instance (state=%v)", name, m.state)
			m.state = StateHome
		}
	}
}

// Every way the online session check can end must release the gate; a path
// that didn't would block launching for the rest of the session.
func TestLaunch_EveryGateOutcomeReleasesTheGate(t *testing.T) {
	cases := map[string]func(inst *core.Instance) tea.Msg{
		"proceed":       func(i *core.Instance) tea.Msg { return ui.ProceedWithLaunch{Instance: i} },
		"needs sign-in": func(i *core.Instance) tea.Msg { return ui.SessionGateFailed{NeedAuth: true, Instance: i} },
		"network error": func(i *core.Instance) tea.Msg { return ui.SessionGateFailed{Err: errors.New("offline")} },
		"refresh rejected": func(i *core.Instance) tea.Msg {
			return sessionRefreshedMsg{launch: i, err: errors.New("invalid_grant"), authError: true}
		},
		"refresh network error":   func(i *core.Instance) tea.Msg { return sessionRefreshedMsg{launch: i, err: errors.New("timeout")} },
		"refresh with no account": func(i *core.Instance) tea.Msg { return launchRefreshTriggerMsg{instance: i} },
	}
	for name, outcome := range cases {
		t.Run(name, func(t *testing.T) {
			m, inst := newLaunchTestModel(t)
			m.Update(ui.NavigateToLaunch{Instance: inst})
			if !m.gatingLaunch {
				t.Fatal("setup: the gate should be in flight")
			}
			// Apply the outcome, following up to a few synchronous follow-up
			// messages (e.g. a refresh result that turns into SessionGateFailed).
			msg := outcome(inst)
			for range 3 {
				_, cmd := m.Update(msg)
				if !m.gatingLaunch || cmd == nil {
					break
				}
				msg = cmd()
			}
			if m.gatingLaunch {
				t.Fatal("the gate is still held")
			}
			m.stopLaunch()
			m.state = StateHome
			if _, cmd := m.Update(ui.NavigateToLaunch{Instance: inst}); cmd == nil || !m.gatingLaunch {
				t.Error("a new launch should be possible afterwards")
			}
		})
	}
}

// ctrl+c with a game running asks for a second press too (it used to skip the
// q guard and quit at once), and the prompt shows on whatever screen is open.
func TestQuit_CtrlCWhileGameRunsNeedsSecondPress(t *testing.T) {
	m, inst := newLaunchTestModel(t)
	playingDetached(t, m, inst) // on the launch screen

	if _, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlC}); cmd != nil || !m.quitArmed {
		t.Fatal("first ctrl+c with a game running should ask, not quit")
	}
	if !strings.Contains(m.View(), "press again to quit") {
		t.Error("the quit prompt should show on the launch screen")
	}
	_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	if cmd == nil {
		t.Fatal("a second ctrl+c should quit")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Error("a second ctrl+c should quit")
	}
}
