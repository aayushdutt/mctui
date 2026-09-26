package app

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/ui"
	tea "github.com/charmbracelet/bubbletea"
)

// A malformed config.json or accounts.json must not crash startup (config used
// to nil-panic on every launch) or be silently overwritten by the next save
// (accounts.json holds refresh tokens): both are moved aside and reported.
func TestNew_RecoversFromUnreadableStateFiles(t *testing.T) {
	xdg := t.TempDir()
	t.Setenv("XDG_DATA_HOME", xdg)
	t.Setenv("APPDATA", "")
	orig := ui.ActiveName()
	t.Cleanup(func() { ui.Apply(orig) })

	dataDir := filepath.Join(xdg, "mctui")
	if err := os.MkdirAll(dataDir, 0o755); err != nil {
		t.Fatal(err)
	}
	writeFile(t, filepath.Join(dataDir, "config.json"), `{"theme": "dark",`)
	writeFile(t, filepath.Join(dataDir, "accounts.json"), `{"accounts": [{"id": "u1"`)

	m := New()

	if m.cfg == nil || m.cfg.DataDir != dataDir {
		t.Fatalf("expected default config rooted at %s, got %+v", dataDir, m.cfg)
	}
	if n := len(m.accounts.Accounts); n != 0 {
		t.Errorf("expected no accounts after a failed load, got %d", n)
	}
	for _, name := range []string{"config.json", "accounts.json"} {
		kept, _ := filepath.Glob(filepath.Join(dataDir, name+".bad-*"))
		if len(kept) != 1 {
			t.Errorf("%s: expected the unreadable file kept as %s.bad-<ts>, found %v", name, name, kept)
		}
		if _, err := os.Stat(filepath.Join(dataDir, name)); !os.IsNotExist(err) {
			t.Errorf("%s: expected the unreadable original moved aside (stat err=%v)", name, err)
		}
	}
	if runtime.GOOS != "windows" {
		kept, _ := filepath.Glob(filepath.Join(dataDir, "accounts.json.bad-*"))
		if fi, err := os.Stat(kept[0]); err == nil && fi.Mode().Perm() != 0o600 {
			t.Errorf("the kept accounts copy holds tokens; perm = %o, want 600", fi.Mode().Perm())
		}
	}
	warnings := strings.Join(m.startupWarnings, "\n")
	for _, want := range []string{"config.json", "accounts.json"} {
		if !strings.Contains(warnings, want) {
			t.Errorf("startup warnings should mention %s, got:\n%s", want, warnings)
		}
	}
}

func writeFile(t *testing.T, path, data string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(data), 0o644); err != nil {
		t.Fatal(err)
	}
}

// A refresh response without a rotated refresh_token (optional in OAuth) must
// keep the stored one; wiping it forced a device-code sign-in on every expiry.
func TestSessionRefreshed_KeepsRefreshTokenWhenNotRotated(t *testing.T) {
	m := newTestModel(t)
	acc := &core.Account{ID: "u1", Name: "Steve", Type: core.AccountTypeMSA, AccessToken: "old-access", MSARefreshToken: "old-refresh"}
	m.accounts.Add(acc)

	m.Update(sessionRefreshedMsg{accountID: "u1", accessToken: "new-access", expiresAt: time.Now().Add(time.Hour)})

	if acc.AccessToken != "new-access" {
		t.Errorf("access token = %q, want the refreshed one", acc.AccessToken)
	}
	if acc.MSARefreshToken != "old-refresh" {
		t.Errorf("refresh token = %q, want the stored one kept", acc.MSARefreshToken)
	}

	m.Update(sessionRefreshedMsg{accountID: "u1", accessToken: "newer-access", refreshToken: "rotated", expiresAt: time.Now().Add(time.Hour)})
	if acc.MSARefreshToken != "rotated" {
		t.Errorf("refresh token = %q, want the rotated one", acc.MSARefreshToken)
	}
}

// Typing q into the home filter used to quit the app (the global quit guard
// only checked overlays, not the filter).
func TestQuit_NotWhileTypingInHomeFilter(t *testing.T) {
	m := newTestModel(t)
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	m.Update(ui.InstancesLoaded{Instances: []*core.Instance{{ID: "a", Name: "Aqua", Version: "1.21.4"}}})

	m.Update(keyRunes("/"))
	_, cmd := m.Update(keyRunes("q"))
	if cmd != nil {
		if _, quit := cmd().(tea.QuitMsg); quit {
			t.Fatal("q while filtering must type into the filter, not quit")
		}
	}

	// Outside the filter, q still quits.
	m.Update(tea.KeyMsg{Type: tea.KeyEsc}) // cancel filtering
	m.Update(tea.KeyMsg{Type: tea.KeyEsc}) // (clear an applied filter, if any)
	_, cmd = m.Update(keyRunes("q"))
	if cmd == nil {
		t.Fatal("q on home should quit")
	}
	if _, quit := cmd().(tea.QuitMsg); !quit {
		t.Error("q on home should quit")
	}
}

// An accounts.json that merely can't be read (e.g. root-owned after a sudo
// run) is not moved aside, and saves are refused so it can't be overwritten.
func TestNew_UnreadableAccountsIsLeftAloneAndNotOverwritten(t *testing.T) {
	if runtime.GOOS == "windows" || os.Geteuid() == 0 {
		t.Skip("needs POSIX permissions and a non-root user")
	}
	xdg := t.TempDir()
	t.Setenv("XDG_DATA_HOME", xdg)
	t.Setenv("APPDATA", "")
	orig := ui.ActiveName()
	t.Cleanup(func() { ui.Apply(orig) })

	dataDir := filepath.Join(xdg, "mctui")
	if err := os.MkdirAll(dataDir, 0o755); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dataDir, "accounts.json")
	writeFile(t, path, `{"accounts": []}`)
	if err := os.Chmod(path, 0o000); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(path, 0o600) })

	m := New()

	if kept, _ := filepath.Glob(path + ".bad-*"); len(kept) != 0 {
		t.Errorf("an unreadable (not malformed) file must stay in place, found %v", kept)
	}
	if err := m.accounts.Save(); err == nil {
		t.Error("saving over an accounts.json that couldn't be read must be refused")
	}
	if len(m.startupWarnings) != 1 || !strings.Contains(m.startupWarnings[0], "accounts.json") {
		t.Errorf("expected one warning about accounts.json, got %q", m.startupWarnings)
	}
}
