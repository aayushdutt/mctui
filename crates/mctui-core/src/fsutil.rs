//! Small filesystem helpers for state files.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Replaces `path` with `data` so that readers, and a crash mid-write, see
/// either the old contents or the new, never a torn file: writes a temp file
/// in the same directory, fsyncs it, then renames it over `path`.
///
/// A symlinked `path` is written through to its target and the link is kept,
/// even when the target doesn't exist yet (a dotfiles link made before the
/// file): the target is created, and if its folder is missing the write fails
/// rather than replacing the link. A new file gets `mode`; an existing file
/// keeps its mode narrowed by `mode`, so `accounts.json` can be tightened to
/// 0600 while a file the user made read-only stays read-only. `mode` is
/// ignored off Unix.
pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
    let path = link_target(path)?;
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let name = path.file_name().unwrap_or_default().to_string_lossy();

    // On any error the temp file is removed when `tmp` drops.
    let mut tmp = tempfile::NamedTempFile::with_prefix_in(format!(".{name}.tmp-"), dir)?;
    tmp.write_all(data)?;
    set_mode(tmp.as_file(), &path, mode)?;
    tmp.as_file().sync_all()?;
    tmp.persist(&path)?;
    Ok(())
}

/// Follows `path` through any chain of symlinks to the file they end at,
/// whether or not it exists. Folder symlinks along the way are left to the OS.
fn link_target(path: &Path) -> io::Result<PathBuf> {
    // As many links as Linux follows resolving one path.
    const MAX_LINKS: usize = 40;
    let mut path = path.to_owned();
    for _ in 0..MAX_LINKS {
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {}
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(path),
            Err(e) => return Err(e),
        }
        let target = fs::read_link(&path)?;
        // A relative target is relative to the link's folder; `join` keeps an
        // absolute one as is.
        path = path.parent().unwrap_or(Path::new("")).join(target);
    }
    Err(io::Error::other(format!(
        "too many levels of symbolic links at {}",
        path.display()
    )))
}

#[cfg(unix)]
fn set_mode(file: &File, target: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = match fs::metadata(target) {
        Ok(existing) => existing.permissions().mode() & mode & 0o777,
        Err(_) => mode,
    };
    file.set_permissions(fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_file: &File, _target: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

/// Renames a malformed file aside as `<name>.bad-YYYYMMDD-HHMMSS` (local time)
/// so a later save can't silently overwrite what it held; returns the new path.
/// Never replaces an earlier one: a second file quarantined in the same second
/// gets `-2`, then `-3`, and so on.
pub fn quarantine(path: &Path) -> io::Result<PathBuf> {
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    quarantine_as(path, &stamp)
}

fn quarantine_as(path: &Path, stamp: &str) -> io::Result<PathBuf> {
    let mut n = 1;
    loop {
        let mut dest = path.as_os_str().to_owned();
        dest.push(format!(".bad-{stamp}"));
        if n > 1 {
            dest.push(format!("-{n}"));
        }
        let dest = PathBuf::from(dest);
        match fs::symlink_metadata(&dest) {
            Ok(_) => n += 1,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                fs::rename(path, &dest)?;
                return Ok(dest);
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn writes_and_replaces_without_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("state.json");
        write_atomic(&p, b"one", 0o644).unwrap();
        write_atomic(&p, b"two", 0o644).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        assert_eq!(names_in(dir.path()), ["state.json"]);
    }

    #[test]
    fn quarantine_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("accounts.json");
        for body in ["first", "second", "third"] {
            fs::write(&p, body).unwrap();
            quarantine_as(&p, "20260927-190000").unwrap();
        }
        assert_eq!(
            names_in(dir.path()),
            [
                "accounts.json.bad-20260927-190000",
                "accounts.json.bad-20260927-190000-2",
                "accounts.json.bad-20260927-190000-3",
            ]
        );
        let read = |name: &str| fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(read("accounts.json.bad-20260927-190000"), "first");
        assert_eq!(read("accounts.json.bad-20260927-190000-3"), "third");
    }

    #[test]
    fn quarantine_moves_file_aside() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        fs::write(&p, "{broken").unwrap();
        let dest = quarantine(&p).unwrap();
        let name = dest.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("config.json.bad-"), "{name}");
        assert_eq!(name.len(), "config.json.bad-20260927-190000".len());
        assert!(!p.exists());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "{broken");
    }

    #[cfg(unix)]
    mod unix {
        use std::os::unix::fs::{PermissionsExt, symlink};

        use super::*;

        fn mode_of(p: &Path) -> u32 {
            fs::metadata(p).unwrap().permissions().mode() & 0o777
        }

        #[test]
        fn new_file_gets_requested_mode() {
            let dir = tempfile::tempdir().unwrap();
            let p = dir.path().join("secrets.json");
            write_atomic(&p, b"x", 0o600).unwrap();
            assert_eq!(mode_of(&p), 0o600);
            let q = dir.path().join("config.json");
            write_atomic(&q, b"x", 0o644).unwrap();
            assert_eq!(mode_of(&q), 0o644);
        }

        #[test]
        fn existing_mode_is_kept_but_narrowed() {
            for (existing, requested, want) in [
                (0o444, 0o644, 0o444),
                (0o600, 0o644, 0o600),
                (0o644, 0o600, 0o600),
            ] {
                let dir = tempfile::tempdir().unwrap();
                let p = dir.path().join("f.json");
                fs::write(&p, "x").unwrap();
                fs::set_permissions(&p, fs::Permissions::from_mode(existing)).unwrap();
                write_atomic(&p, b"y", requested).unwrap();
                assert_eq!(
                    mode_of(&p),
                    want,
                    "existing {existing:o}, requested {requested:o}"
                );
                assert_eq!(fs::read(&p).unwrap(), b"y");
            }
        }

        #[test]
        fn writes_through_symlink() {
            let dir = tempfile::tempdir().unwrap();
            let real = dir.path().join("real.json");
            let link = dir.path().join("config.json");
            fs::write(&real, "old").unwrap();
            symlink(&real, &link).unwrap();
            write_atomic(&link, b"new", 0o644).unwrap();
            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        }

        #[test]
        fn writes_through_a_chain_of_relative_symlinks() {
            let dir = tempfile::tempdir().unwrap();
            fs::create_dir(dir.path().join("dotfiles")).unwrap();
            let real = dir.path().join("dotfiles/real.json");
            let hop = dir.path().join("dotfiles/hop.json");
            let link = dir.path().join("config.json");
            fs::write(&real, "old").unwrap();
            symlink("real.json", &hop).unwrap();
            symlink("dotfiles/hop.json", &link).unwrap();
            write_atomic(&link, b"new", 0o644).unwrap();
            assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
            assert!(fs::symlink_metadata(&hop).unwrap().is_symlink());
            assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        }

        #[test]
        fn dangling_symlink_is_kept_and_its_target_created() {
            let dir = tempfile::tempdir().unwrap();
            let real = dir.path().join("real.json");
            let link = dir.path().join("config.json");
            symlink(&real, &link).unwrap();
            write_atomic(&link, b"new", 0o600).unwrap();
            assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
            assert_eq!(fs::read_to_string(&real).unwrap(), "new");
            assert_eq!(mode_of(&real), 0o600);
        }

        #[test]
        fn symlink_into_a_missing_folder_fails_and_keeps_the_link() {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("gone/real.json");
            let link = dir.path().join("config.json");
            symlink(&target, &link).unwrap();
            let err = write_atomic(&link, b"new", 0o644).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::NotFound, "{err}");
            assert_eq!(fs::read_link(&link).unwrap(), target);
            assert_eq!(names_in(dir.path()), ["config.json"]);
        }

        #[test]
        fn symlink_loop_is_an_error() {
            let dir = tempfile::tempdir().unwrap();
            let a = dir.path().join("a.json");
            let b = dir.path().join("b.json");
            symlink(&b, &a).unwrap();
            symlink(&a, &b).unwrap();
            assert!(write_atomic(&a, b"x", 0o644).is_err());
            assert!(fs::symlink_metadata(&a).unwrap().is_symlink());
        }

        #[test]
        fn failure_leaves_target_intact() {
            let dir = tempfile::tempdir().unwrap();
            let p = dir.path().join("state.json");
            fs::write(&p, "original").unwrap();
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
            let result = write_atomic(&p, b"new", 0o644);
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
            if result.is_ok() {
                return; // running as root: directory permissions don't apply
            }
            assert_eq!(fs::read_to_string(&p).unwrap(), "original");
            assert_eq!(names_in(dir.path()), ["state.json"]);
        }
    }
}
