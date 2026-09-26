package ui

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/x/ansi"
)

// If sign-in succeeds but the account can't be saved, the success card must say
// so (the session won't survive a restart) instead of auto-closing over it.
func TestAuth_SaveFailureIsShownNotSwallowed(t *testing.T) {
	restoreDark(t)
	// A data dir that doesn't exist makes the atomic save fail.
	mgr := core.NewAccountManager(filepath.Join(t.TempDir(), "missing", "dir"))
	m := NewAuthModel("", "client", mgr, nil)
	m.SetSize(100, 30)

	_, cmd := m.Update(accountCreatedMsg{acc: &core.Account{ID: "u1", Name: "Steve"}})

	if cmd != nil {
		t.Error("a failed save must not schedule the auto-return to home")
	}
	view := ansi.Strip(m.View())
	if !strings.Contains(view, "Couldn't save the account") {
		t.Errorf("expected the save error on the success card, got:\n%s", view)
	}
	if len(mgr.Accounts) != 1 {
		t.Errorf("the account should still be usable this session")
	}
}
