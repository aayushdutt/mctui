// Package fsutil holds small filesystem helpers shared across packages.
package fsutil

import (
	"fmt"
	"os"
	"path/filepath"
	"time"
)

// WriteFileAtomic replaces path with data so that readers — and a crash or
// power loss mid-write — see either the old contents or the new, never a torn
// file. It writes a temp file in the same directory (so the final rename stays
// on one filesystem), fsyncs it, sets its mode, then renames it over path.
//
// A symlinked path is written through to its target (the link is kept). A new
// file gets perm; an existing file keeps its mode, narrowed by perm — so
// accounts.json can be tightened to 0600 while a file the user made read-only
// stays read-only.
func WriteFileAtomic(path string, data []byte, perm os.FileMode) (err error) {
	if target, err := filepath.EvalSymlinks(path); err == nil {
		path = target
	}
	mode := perm
	if fi, err := os.Stat(path); err == nil {
		mode = fi.Mode().Perm() & perm
	}

	f, err := os.CreateTemp(filepath.Dir(path), "."+filepath.Base(path)+".tmp-*")
	if err != nil {
		return err
	}
	tmp := f.Name()
	defer func() {
		if err != nil {
			_ = f.Close()
			_ = os.Remove(tmp)
		}
	}()

	if _, err = f.Write(data); err != nil {
		return err
	}
	if err = f.Chmod(mode); err != nil {
		return err
	}
	if err = f.Sync(); err != nil {
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// Quarantine moves an unreadable or malformed file aside to
// "<path>.bad-<timestamp>" so a later save can't silently overwrite whatever it
// held, and returns the new path.
func Quarantine(path string) (string, error) {
	dest := fmt.Sprintf("%s.bad-%s", path, time.Now().Format("20060102-150405"))
	if err := os.Rename(path, dest); err != nil {
		return "", err
	}
	return dest, nil
}
