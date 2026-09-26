package fsutil

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestWriteFileAtomic_WritesAndReplaces(t *testing.T) {
	p := filepath.Join(t.TempDir(), "state.json")

	if err := WriteFileAtomic(p, []byte("one"), 0o644); err != nil {
		t.Fatalf("first write: %v", err)
	}
	if err := WriteFileAtomic(p, []byte("two"), 0o644); err != nil {
		t.Fatalf("second write: %v", err)
	}
	got, err := os.ReadFile(p)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != "two" {
		t.Errorf("contents = %q, want %q", got, "two")
	}

	// No temp files are left behind in the directory.
	entries, err := os.ReadDir(filepath.Dir(p))
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 1 {
		var names []string
		for _, e := range entries {
			names = append(names, e.Name())
		}
		t.Errorf("dir entries = %v, want only state.json", names)
	}
}

func TestWriteFileAtomic_AppliesAndTightensPerm(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("POSIX permission bits")
	}
	p := filepath.Join(t.TempDir(), "secrets.json")
	if err := os.WriteFile(p, []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := WriteFileAtomic(p, []byte("new"), 0o600); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Stat(p)
	if err != nil {
		t.Fatal(err)
	}
	if perm := fi.Mode().Perm(); perm != 0o600 {
		t.Errorf("perm = %o, want 600", perm)
	}
}

// A write that fails part-way (here: the temp file can't be created in a
// read-only directory) must leave the existing target untouched.
func TestWriteFileAtomic_FailureLeavesTargetIntact(t *testing.T) {
	if runtime.GOOS == "windows" || os.Geteuid() == 0 {
		t.Skip("needs POSIX directory permissions and a non-root user")
	}
	dir := t.TempDir()
	p := filepath.Join(dir, "state.json")
	if err := os.WriteFile(p, []byte("original"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(dir, 0o555); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(dir, 0o755) })

	if err := WriteFileAtomic(p, []byte("new"), 0o644); err == nil {
		t.Fatal("expected the write to fail in a read-only directory")
	}
	if got, _ := os.ReadFile(p); string(got) != "original" {
		t.Errorf("target changed after a failed write: %q", got)
	}
	entries, _ := os.ReadDir(dir)
	for _, e := range entries {
		if strings.Contains(e.Name(), ".tmp-") {
			t.Errorf("temp file left behind: %s", e.Name())
		}
	}
}

// A symlinked file (e.g. a config kept in a dotfiles repo) is written through:
// the link survives and its target gets the new contents.
func TestWriteFileAtomic_WritesThroughSymlink(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlinks need privileges on Windows")
	}
	dir := t.TempDir()
	real := filepath.Join(dir, "real.json")
	link := filepath.Join(dir, "config.json")
	if err := os.WriteFile(real, []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(real, link); err != nil {
		t.Fatal(err)
	}
	if err := WriteFileAtomic(link, []byte("new"), 0o644); err != nil {
		t.Fatal(err)
	}
	if fi, err := os.Lstat(link); err != nil || fi.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("the symlink was replaced (err=%v)", err)
	}
	if got, _ := os.ReadFile(real); string(got) != "new" {
		t.Errorf("link target = %q, want new", got)
	}
}

// An existing file keeps its mode (narrowed by perm): a read-only file stays
// read-only; a private one isn't loosened.
func TestWriteFileAtomic_KeepsExistingMode(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("POSIX permission bits")
	}
	for _, tc := range []struct{ existing, perm, want os.FileMode }{
		{0o444, 0o644, 0o444},
		{0o600, 0o644, 0o600},
		{0o644, 0o600, 0o600},
	} {
		p := filepath.Join(t.TempDir(), "f.json")
		if err := os.WriteFile(p, []byte("x"), tc.existing); err != nil {
			t.Fatal(err)
		}
		if err := os.Chmod(p, tc.existing); err != nil {
			t.Fatal(err)
		}
		if err := WriteFileAtomic(p, []byte("y"), tc.perm); err != nil {
			t.Fatal(err)
		}
		fi, _ := os.Stat(p)
		if got := fi.Mode().Perm(); got != tc.want {
			t.Errorf("existing %o, perm %o: got %o, want %o", tc.existing, tc.perm, got, tc.want)
		}
	}
}

func TestQuarantine_MovesFileAside(t *testing.T) {
	p := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(p, []byte("{broken"), 0o644); err != nil {
		t.Fatal(err)
	}
	dest, err := Quarantine(p)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(filepath.Base(dest), "config.json.bad-") {
		t.Errorf("dest = %q, want config.json.bad-<ts>", dest)
	}
	if _, err := os.Stat(p); !os.IsNotExist(err) {
		t.Errorf("original still present (err=%v)", err)
	}
	if got, _ := os.ReadFile(dest); string(got) != "{broken" {
		t.Errorf("quarantined contents = %q", got)
	}
}
