//! Launching an instance and watching the game run.
//!
//! A launch goes in stages, each usable on its own:
//!
//! 1. [`plan`] works out, without touching the disk or network, the command
//!    line and every file it needs, from the version's JSON.
//! 2. [`prepare`] downloads those files (checksums verified), unpacks native
//!    libraries, and lays out old versions' assets. An instance already
//!    fully downloaded skips the network, after checking its files are there.
//! 3. [`Game::spawn`] starts Java and hands back a [`Game`] to stop or wait
//!    for, and its [`Logs`].
//!
//! [`Launcher::launch`] runs the lot in order: version metadata, Java,
//! starter mods, plan, prepare, start. It reports each [`Step`] as it goes,
//! and what the instance should remember as [`InstanceChanges`]; saving is
//! up to the caller. Dropping its future before the game starts cancels the
//! launch without leaving partial files behind.

mod game;
mod identity;
mod launcher;
mod log;
mod natives;
mod plan;
mod prepare;
mod user_args;

use std::io;
use std::path::PathBuf;

pub use game::{DEFAULT_STOP_GRACE, Exit, ExitReport, Game, Logs, Stopped};
pub use identity::{Identity, offline_uuid};
pub use launcher::{InstanceChanges, Launched, Launcher};
pub use log::{Level, LogLine, Stream, classify};
pub use plan::{AssetLayout, Assets, NativeJar, Plan, PlanInputs, plan};
pub use prepare::{Prepared, RESOURCES_URL, prepare};

use crate::java::{self, InstallProgress};
use crate::modrinth::StarterProgress;
use crate::mojang::maven::InvalidCoordinate;
use crate::net::{self, Progress};
use crate::{fabric, mojang};

/// Why a launch failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The game version's metadata couldn't be had.
    #[error(transparent)]
    Version(#[from] mojang::Error),
    /// The Fabric loader's metadata couldn't be had.
    #[error(transparent)]
    Fabric(#[from] fabric::Error),
    /// Fabric has no loader for this game version.
    #[error("Fabric isn't available for Minecraft {0}")]
    NoFabricLoader(String),
    /// The instance uses a mod loader mctui can't launch.
    #[error("mctui can't launch {0} instances yet")]
    UnsupportedLoader(String),
    /// No Java could be found or installed.
    #[error(transparent)]
    Java(#[from] java::Error),
    /// The version's metadata has no client jar to download.
    #[error("Minecraft {0} has no game download")]
    NoClient(String),
    /// A library's name isn't a Maven coordinate, so its file is unknown.
    #[error("Minecraft {version} lists a library mctui can't find: {source}")]
    BadLibrary {
        version: String,
        #[source]
        source: InvalidCoordinate,
    },
    /// The metadata names a file that would land outside its folder.
    #[error("the game's metadata names an unsafe file path: {0:?}")]
    UnsafePath(String),
    /// A folder the game needs has a name that isn't UTF-8, which Java's
    /// arguments can't carry.
    #[error("{} has a name mctui can't pass to Java; rename it", .0.display())]
    NonUtf8Path(PathBuf),
    /// A download failed, including a bad checksum.
    #[error(transparent)]
    Download(#[from] net::Error),
    /// The downloaded list of assets isn't what it should be.
    #[error("the asset list {} is damaged: {source}", path.display())]
    AssetIndex {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// A natives jar couldn't be unpacked.
    #[error("couldn't unpack {}: {source}", jar.display())]
    Natives {
        jar: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A natives jar holds a file that would land outside the natives folder.
    #[error("{} holds an unsafe file path: {entry:?}", jar.display())]
    UnsafeNatives { jar: PathBuf, entry: String },
    /// Reading or writing the game's files failed.
    #[error("couldn't set up {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Java couldn't be started.
    #[error("couldn't start Java ({}): {source}", java.display())]
    Spawn {
        java: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The app is shutting down.
    #[error("the launch was cancelled")]
    Cancelled,
}

impl Error {
    fn io(path: impl Into<PathBuf>) -> impl FnOnce(io::Error) -> Self {
        let path = path.into();
        move |source| Error::Io { path, source }
    }
}

/// A result with this module's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// How far a launch has got, for a step list with a progress bar. Steps come
/// in [`Stage`] order; a stage may be skipped (no starter mods to install,
/// Java already there) or repeat with new progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Getting the version's metadata from Mojang, and from Fabric.
    Version,
    /// Looking for a Java that can run the version.
    Java,
    /// Installing Java, since none installed fits.
    InstallingJava(InstallProgress),
    /// Installing the starter mods of a new Fabric instance.
    StarterMods(StarterProgress),
    /// Fetching the game, its libraries, and its logging setup; or, for an
    /// instance already downloaded, checking they're all still there.
    Libraries(Progress),
    /// Fetching the assets: sounds, languages, icons.
    Assets(Progress),
    /// Unpacking native libraries.
    Natives,
    /// Starting Java.
    Starting,
}

/// The stages of a launch, in order: what a step list shows one row for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// [`Step::Version`].
    Version,
    /// [`Step::Java`] and [`Step::InstallingJava`].
    Java,
    /// [`Step::StarterMods`].
    StarterMods,
    /// [`Step::Libraries`].
    Libraries,
    /// [`Step::Assets`].
    Assets,
    /// [`Step::Natives`].
    Natives,
    /// [`Step::Starting`].
    Starting,
}

impl Step {
    /// The stage this step belongs to.
    pub fn stage(&self) -> Stage {
        match self {
            Step::Version => Stage::Version,
            Step::Java | Step::InstallingJava(_) => Stage::Java,
            Step::StarterMods(_) => Stage::StarterMods,
            Step::Libraries(_) => Stage::Libraries,
            Step::Assets(_) => Stage::Assets,
            Step::Natives => Stage::Natives,
            Step::Starting => Stage::Starting,
        }
    }
}

/// Runs blocking filesystem work on tokio's blocking pool. Its panics are
/// passed on. Dropping the future doesn't stop the work: it runs to the end,
/// so it must leave things whole whenever it stops.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        // Only when the runtime is shutting down.
        Err(_) => Err(Error::Cancelled),
    }
}
