//! Mods from Modrinth: search, pick a version, install it with its required
//! dependencies into an instance's `mods` folder, and list or remove what's
//! installed.
//!
//! Installs are recorded in `mods/.mctui-modrinth.json`, the file the Go app
//! kept, so both read each other's records. Every request goes through
//! [`Http`](crate::net::Http), which retries transient failures and 429s.

use std::io;
use std::path::{Path, PathBuf};

use crate::instance::Instance;
use crate::net;

mod catalog;
mod client;
mod install;
mod installed;
mod model;
mod resolve;
mod starter;

pub use client::{API_URL, Client};
pub use install::{AddedMod, InstallReport};
pub use installed::{InstalledMod, installed, remove};
pub use model::{
    Dependency, DependencyType, FileHashes, Project, SearchHit, SearchResults, Side, Version,
    VersionFile, VersionType, choose_version,
};
pub use resolve::{SkipReason, SkippedDep, UnresolvedDep, UnresolvedReason};
pub use starter::{STARTER_MODS, StarterMod, StarterOutcome, StarterProgress, StarterReport};

/// Why a Modrinth operation failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Net(#[from] net::Error),
    #[error("{project} has no version for Minecraft {game_version} with {loader}")]
    NoCompatibleVersion {
        project: String,
        game_version: String,
        loader: String,
    },
    #[error("mods need a mod loader, and this instance is vanilla")]
    NoModLoader,
    #[error("{0:?} isn't a mod jar")]
    NotAJar(String),
    #[error("couldn't update {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Error {
    fn io(path: &Path) -> impl FnOnce(io::Error) -> Self {
        move |source| Error::Io {
            path: path.to_owned(),
            source,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn mods_dir(inst: &Instance) -> PathBuf {
    inst.game_dir().join("mods")
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Runs blocking filesystem work on tokio's blocking pool. Its panics are
/// passed on; it's only cancelled when the runtime is shutting down.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        Err(e) => Err(io::Error::other(e)),
    }
}
