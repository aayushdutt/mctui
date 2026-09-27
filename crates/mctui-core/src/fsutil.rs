//! Small filesystem helpers: atomic writes and locks for state files,
//! quarantine, safe names, hashing, and running blocking file work off the
//! async runtime.

use std::fs::{self, File};
use std::io::{self, Read as _, Write};
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
    fsync(tmp.as_file())?;
    tmp.persist(&path)?;
    Ok(())
}

/// Flushes `file` to the disk before it's renamed into place. A plain
/// `fsync` on Unix: `sync_all` is a full cache flush on macOS
/// (`F_FULLFSYNC`), which settings saves from a key press can't wait for.
#[cfg(unix)]
fn fsync(file: &File) -> io::Result<()> {
    rustix::fs::fsync(file).map_err(io::Error::from)
}

#[cfg(not(unix))]
fn fsync(file: &File) -> io::Result<()> {
    file.sync_all()
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

/// Renames folder `new` to `dest`, replacing any folder there. The old
/// folder is moved into a scratch folder in `scratch` first and deleted
/// after, so `dest` is always one whole folder or absent, never a mix. If
/// the new folder can't be moved in, the old one is put back; if even that
/// fails, it's left in the scratch folder rather than deleted. `new`,
/// `dest`, and `scratch` must be on one filesystem.
pub(crate) fn replace_dir(new: &Path, dest: &Path, scratch: &Path) -> io::Result<()> {
    replace_dir_with(new, dest, scratch, |from, to| fs::rename(from, to))
}

/// [`replace_dir`], renaming with `rename` (tests make it fail).
fn replace_dir_with(
    new: &Path,
    dest: &Path,
    scratch: &Path,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let trash = tempfile::Builder::new()
        .prefix(".old-")
        .tempdir_in(scratch)?;
    let old = trash.path().join("old");
    let had_old = match rename(dest, &old) {
        Ok(()) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    };
    if let Err(e) = rename(new, dest) {
        if had_old && let Err(restore) = rename(&old, dest) {
            // Dropping `trash` would delete the only copy left.
            let kept = trash.keep();
            tracing::warn!(
                folder = %dest.display(),
                kept = %kept.display(),
                error = %restore,
                "couldn't put the old folder back; it's kept aside"
            );
        }
        return Err(e);
    }
    // Dropping `trash` deletes the old folder.
    Ok(())
}

/// An advisory lock on a lock file, held until dropped. Every mctui that
/// changes a state file takes the same lock first, so read-change-write
/// updates from two windows can't lose each other's changes.
#[derive(Debug)]
pub(crate) struct FileLock {
    _file: File,
}

impl FileLock {
    /// Waits for the lock on `path`, creating the file if it's missing. The
    /// file is never deleted: two writers could then lock different files.
    pub(crate) fn acquire(path: &Path) -> io::Result<Self> {
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        file.lock()?;
        Ok(Self { _file: file })
    }
}

/// Whether `name` is a plain file or folder name, safe to join onto a
/// folder: not empty, not `.` or `..`, and without separators (`/`, `\`), a
/// drive colon, or control characters. Guards paths built from names that
/// come from servers or files.
pub(crate) fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(|c: char| matches!(c, '/' | '\\' | ':') || c.is_control())
}

/// `bytes` as lowercase hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// The `D` digest (SHA-1, SHA-256) of the file at `path` as lowercase hex,
/// read a chunk at a time. Blocking.
pub(crate) fn hash_file<D: sha2::Digest>(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = D::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

/// Runs blocking file work on tokio's blocking pool. Its panics are passed
/// on. Dropping the future doesn't stop the work: it runs to the end, so it
/// must leave things whole whenever it stops. Fails by itself only when the
/// runtime is shutting down, with [`ShuttingDown`].
pub(crate) async fn blocking<T, E>(
    work: impl FnOnce() -> Result<T, E> + Send + 'static,
) -> Result<T, E>
where
    T: Send + 'static,
    E: From<ShuttingDown> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        Err(_) => Err(ShuttingDown.into()),
    }
}

/// [`blocking`] work never ran: the runtime is shutting down.
#[derive(Debug)]
pub(crate) struct ShuttingDown;

impl From<ShuttingDown> for io::Error {
    fn from(_: ShuttingDown) -> Self {
        io::Error::other("the app is shutting down")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_a_folder_swaps_it_whole() {
        let root = tempfile::tempdir().unwrap();
        let (new, dest) = (root.path().join("new"), root.path().join("21"));
        fs::create_dir_all(dest.join("old-only")).unwrap();
        fs::create_dir_all(new.join("bin")).unwrap();

        replace_dir(&new, &dest, root.path()).unwrap();
        assert!(dest.join("bin").is_dir());
        assert!(!dest.join("old-only").exists());
        assert_eq!(names_in(root.path()), ["21"], "no scratch folders left");
    }

    #[test]
    fn a_failed_replace_keeps_the_old_folder() {
        let root = tempfile::tempdir().unwrap();
        let (new, dest) = (root.path().join("new"), root.path().join("21"));
        fs::create_dir_all(dest.join("old-only")).unwrap();
        fs::create_dir_all(&new).unwrap();

        // Moving the new folder in fails: the old one is put back.
        let refuse_new = |from: &Path, to: &Path| {
            if from == new {
                return Err(io::Error::other("refused"));
            }
            fs::rename(from, to)
        };
        replace_dir_with(&new, &dest, root.path(), refuse_new).unwrap_err();
        assert!(dest.join("old-only").is_dir());
        assert_eq!(names_in(root.path()), ["21", "new"]);

        // Putting it back fails too: it's kept aside, not deleted.
        let first = std::cell::Cell::new(true);
        let refuse_after_one = |from: &Path, to: &Path| {
            if first.replace(false) {
                fs::rename(from, to)
            } else {
                Err(io::Error::other("refused"))
            }
        };
        replace_dir_with(&new, &dest, root.path(), refuse_after_one).unwrap_err();
        assert!(!dest.exists());
        let kept: Vec<_> = names_in(root.path())
            .into_iter()
            .filter(|name| name.starts_with(".old-"))
            .collect();
        let [kept] = kept.as_slice() else {
            panic!("want the scratch folder kept, got {kept:?}");
        };
        assert!(root.path().join(kept).join("old/old-only").is_dir());
    }

    #[test]
    fn plain_names() {
        for good in ["1.21.8", "My Pack (2)", "café", "a..b", ".hidden"] {
            assert!(is_plain_name(good), "{good:?}");
        }
        for bad in ["", ".", "..", "a/b", "a\\b", "C:", "/", "nul\0", "tab\t"] {
            assert!(!is_plain_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn hashes_files_as_hex() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello");
        fs::write(&path, "hello").unwrap();
        assert_eq!(
            hash_file::<sha1::Sha1>(&path).unwrap(),
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"
        );
        assert_eq!(
            hash_file::<sha2::Sha256>(&path).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(hex(&[0, 0xab, 0xff]), "00abff");
    }

    #[test]
    fn locks_are_exclusive_until_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".state.lock");
        let held = FileLock::acquire(&path).unwrap();
        let other = File::open(&path).unwrap();
        assert!(other.try_lock().is_err(), "held by the first");
        drop(held);
        other.try_lock().unwrap();
    }

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
