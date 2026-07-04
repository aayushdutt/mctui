package ui

import (
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
)

// homeWithAccount builds a home model with one active account of the given type.
// An empty accType means no account manager is configured at all (the
// GetActive() == nil path).
func homeWithAccount(t *testing.T, accType core.AccountType) *HomeModel {
	t.Helper()
	m := newHomeWith(t, fabricInstance(t))
	if accType == "" {
		return m
	}
	am := core.NewAccountManager(t.TempDir())
	am.Add(&core.Account{ID: "acc", Name: "Tester", Type: accType})
	if err := am.SetActive("acc"); err != nil {
		t.Fatal(err)
	}
	m.SetAccountManager(am)
	return m
}

// TestLaunchSelectedRouting pins the launch-routing decision shared by Enter, the
// `o` key, and the Overview Play button. The rules: an offline launch always goes
// straight to the launch screen; an online launch routes to auth unless there is
// a usable session. The MSA invalid / uncertain rows are the important guard —
// a stale or unverifiable session must re-auth rather than attempt a doomed
// online launch — and launchSelected must agree with noOnlineSession (the
// predicate that drives the "Login & play" label) on every row.
func TestLaunchSelectedRouting(t *testing.T) {
	restoreDark(t)

	type want int
	const (
		wantLaunchOnline want = iota
		wantLaunchOffline
		wantAuth
	)

	cases := []struct {
		name                string
		accType             core.AccountType // "" → no account manager
		session             ActiveSessionCheckStatus
		offline             bool
		want                want
		wantNoOnlineSession bool
	}{
		{"offline flag, no account", "", ActiveSessionNotApplicable, true, wantLaunchOffline, true},
		{"offline flag, MSA invalid", core.AccountTypeMSA, ActiveSessionInvalid, true, wantLaunchOffline, true},
		{"online, no account", "", ActiveSessionNotApplicable, false, wantAuth, true},
		{"online, offline account", core.AccountTypeOffline, ActiveSessionNotApplicable, false, wantLaunchOnline, false},
		{"online, MSA session ok", core.AccountTypeMSA, ActiveSessionOK, false, wantLaunchOnline, false},
		{"online, MSA session invalid", core.AccountTypeMSA, ActiveSessionInvalid, false, wantAuth, true},
		{"online, MSA session uncertain", core.AccountTypeMSA, ActiveSessionUncertain, false, wantAuth, true},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			m := homeWithAccount(t, c.accType)
			m.ApplyActiveSessionCheckResult(ActiveSessionCheckResult{Status: c.session})

			// The label predicate and the action must reason about "can launch
			// online" identically.
			if got := m.noOnlineSession(); got != c.wantNoOnlineSession {
				t.Errorf("noOnlineSession() = %v, want %v", got, c.wantNoOnlineSession)
			}

			cmd := m.launchSelected(c.offline)
			if cmd == nil {
				t.Fatal("launchSelected returned nil with an instance selected")
			}
			switch msg := cmd().(type) {
			case NavigateToLaunch:
				if c.want == wantAuth {
					t.Fatalf("got NavigateToLaunch, want NavigateToAuth")
				}
				wantOffline := c.want == wantLaunchOffline
				if msg.Offline != wantOffline {
					t.Errorf("NavigateToLaunch.Offline = %v, want %v", msg.Offline, wantOffline)
				}
				if msg.Instance == nil {
					t.Error("NavigateToLaunch carried a nil instance")
				}
			case NavigateToAuth:
				if c.want != wantAuth {
					t.Fatalf("got NavigateToAuth, want a launch")
				}
			default:
				t.Fatalf("unexpected message type %T", msg)
			}
		})
	}
}

// TestLaunchSelectedNoSelection verifies launchSelected is a no-op (nil cmd) when
// nothing is selected, so Enter / Play on an empty list can't navigate anywhere.
func TestLaunchSelectedNoSelection(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, nil)
	if cmd := m.launchSelected(false); cmd != nil {
		t.Fatalf("expected nil cmd with no selection, got %T", cmd())
	}
	if cmd := m.launchSelected(true); cmd != nil {
		t.Fatalf("expected nil cmd (offline) with no selection, got %T", cmd())
	}
}
