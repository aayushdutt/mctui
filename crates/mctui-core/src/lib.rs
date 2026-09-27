//! Domain model and on-disk storage for mctui: the data dir, `config.json`,
//! instances, and `accounts.json`.
//!
//! Every file format here is shared with the Go launcher this crate replaces,
//! so existing users keep their instances and accounts: readers accept what Go
//! wrote, and writers emit the same key names and shapes.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod account;
pub mod auth;
pub mod config;
pub mod fabric;
pub mod facets;
pub mod fsutil;
pub mod instance;
pub mod java;
pub mod modrinth;
pub mod mojang;
pub mod net;
pub mod paths;
pub mod vanilla_tweaks;

mod gotime;

use std::io;
use std::path::{Path, PathBuf};

pub use jiff::Timestamp;

/// A value loaded from disk plus a user-facing notice when something went
/// wrong but we recovered (file quarantined, defaults used).
#[derive(Debug)]
pub struct Loaded<T> {
    pub value: T,
    pub notice: Option<String>,
}

impl<T> Loaded<T> {
    fn clean(value: T) -> Self {
        Self {
            value,
            notice: None,
        }
    }

    /// Adds `notice` after any notice already there, never replacing it.
    fn add_notice(&mut self, notice: String) {
        self.notice = Some(match self.notice.take() {
            Some(earlier) => format!("{earlier} {notice}"),
            None => notice,
        });
    }
}

/// Reads a state file; `Ok(None)` when it doesn't exist.
fn read_optional(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// The notice for a state file that exists but couldn't be read. The file is
/// left where it is.
fn unreadable_notice(path: &Path, err: &io::Error, consequence: &str) -> String {
    format!("Couldn't read {} ({err}) — {consequence}.", file_name(path))
}

/// Moves a malformed state file aside so a later save can't destroy what it
/// held. Returns the notice and, if the move worked, where the file went.
fn quarantine_malformed(
    path: &Path,
    err: &serde_json::Error,
    consequence: &str,
) -> (String, Option<PathBuf>) {
    let name = file_name(path);
    match fsutil::quarantine(path) {
        Ok(dest) => {
            let notice = format!(
                "{name} was damaged — {consequence}. The old file was kept as {}.",
                file_name(&dest)
            );
            (notice, Some(dest))
        }
        Err(move_err) => {
            tracing::warn!(
                path = %path.display(),
                error = %move_err,
                "couldn't quarantine malformed file"
            );
            (format!("{name} is damaged ({err}) — {consequence}."), None)
        }
    }
}

/// Go wrote "no path" as `""`.
fn optional_path(s: String) -> Option<PathBuf> {
    (!s.is_empty()).then(|| PathBuf::from(s))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_are_combined_not_replaced() {
        let mut loaded = Loaded::clean(());
        loaded.add_notice("First.".to_owned());
        assert_eq!(loaded.notice.as_deref(), Some("First."));
        loaded.add_notice("Second.".to_owned());
        assert_eq!(loaded.notice.as_deref(), Some("First. Second."));
    }
}
