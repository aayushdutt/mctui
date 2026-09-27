//! Java runtimes: find one that can run a Minecraft version, or install one
//! from Eclipse Adoptium.
//!
//! [`Runtimes::find`] picks the Java for a version: the one set in the
//! settings if it's new enough, else an installed runtime of the exact major
//! version (mctui's own first) or, from Java 16 up, the closest newer one.
//! It never picks an older Java. When nothing fits, [`Runtimes::install`]
//! downloads a checksum-verified JRE into mctui's runtimes folder.

mod adoptium;
mod detect;
mod extract;

use std::cmp::Reverse;
use std::io;
use std::path::{Path, PathBuf};

use futures_util::future::join_all;

pub use adoptium::ADOPTIUM_URL;

use crate::config::Config;
use crate::instance::Instance;
use crate::net::{self, Download, Hash, Http};
use crate::{fsutil, paths};

/// The Java a version needs when its JSON doesn't say: before 1.17 the
/// version JSON had no `javaVersion`, and those versions run on Java 8.
pub const DEFAULT_MAJOR: u32 = 8;

/// Versions needing this Java or newer also run on anything newer. Older
/// ones (Java 8) often don't, so they only get an exact match.
const NEWER_OK_FROM: u32 = 16;

/// What can go wrong finding or installing Java.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The Java set in the settings doesn't exist.
    #[error("the Java set in settings ({}) doesn't exist", path.display())]
    ExplicitMissing { path: PathBuf },
    /// The Java set in the settings exists but doesn't run.
    #[error("the Java set in settings ({}) doesn't run", path.display())]
    ExplicitBroken { path: PathBuf },
    /// The Java set in the settings is older than the version needs.
    #[error("the Java set in settings is Java {found}, but this version needs Java {required}")]
    ExplicitTooOld {
        path: PathBuf,
        found: u32,
        required: u32,
    },
    /// There's no config folder to keep runtimes in.
    #[error("couldn't find a folder to keep Java in")]
    NoRuntimesDir,
    /// Adoptium doesn't build Java for this OS or CPU.
    #[error("Java downloads aren't available for {os} on {arch}")]
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },
    /// Adoptium has no release of the needed Java for this system.
    #[error("Adoptium has no Java {major} for this system")]
    Unavailable { major: u32 },
    /// Reaching Adoptium or downloading failed, including a bad checksum.
    #[error(transparent)]
    Net(#[from] net::Error),
    /// The download holds a path that would land outside its folder.
    #[error("the Java download has an unsafe path: {}", entry.display())]
    UnsafeArchive { entry: PathBuf },
    /// The download unpacked, but isn't the Java it should be.
    #[error("the Java {major} download isn't a Java {major} runtime")]
    BadDownload { major: u32 },
    /// Unpacking or moving the runtime into place failed.
    #[error("couldn't install Java: {0}")]
    Io(#[from] io::Error),
}

/// An installed Java.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    /// The executable to launch (`java`, or `java.exe` on Windows).
    pub java: PathBuf,
    /// The major version: 8, 17, 21, ….
    pub major: u32,
    /// Where it was found.
    pub source: Source,
}

/// Where a runtime was found. The order is the search order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// Set in the instance's or the app's settings.
    Explicit,
    /// Installed by mctui (or the Go app) into its runtimes folder.
    Managed,
    /// `$JAVA_HOME`.
    JavaHome,
    /// A `java` on `$PATH`.
    Path,
    /// A usual install folder: `/Library/Java`, `/usr/lib/jvm`, SDKMAN, ….
    System,
}

/// How far [`Runtimes::install`] has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallProgress {
    /// Downloading Java `major`: `done` of `total` bytes.
    Downloading { major: u32, done: u64, total: u64 },
    /// Unpacking Java `major`.
    Extracting { major: u32 },
}

/// The Java major version a Minecraft version needs, from its JSON's
/// `javaVersion.majorVersion`; [`DEFAULT_MAJOR`] when that's absent.
pub fn required_major(declared: Option<u32>) -> u32 {
    declared.filter(|&major| major > 0).unwrap_or(DEFAULT_MAJOR)
}

/// The Java the user set for `instance`: its own, else the app-wide one.
pub fn explicit_path<'a>(instance: &'a Instance, config: &'a Config) -> Option<&'a Path> {
    instance
        .java_path
        .as_deref()
        .or(config.java_path.as_deref())
}

/// Whether an automatic pick of Java `major` can run a version needing
/// `required`.
fn fits(required: u32, major: u32) -> bool {
    major == required || (required >= NEWER_OK_FROM && major > required)
}

/// Where runtimes are looked for and installed, and the Adoptium API.
/// [`Runtimes::system`] fills it in for this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtimes {
    /// mctui's own runtimes, one folder per major version: `<managed>/21`.
    pub managed: PathBuf,
    /// `$JAVA_HOME`.
    pub java_home: Option<PathBuf>,
    /// The folders on `$PATH`.
    pub path: Vec<PathBuf>,
    /// Folders whose subfolders are runtimes, like `/usr/lib/jvm`.
    pub roots: Vec<PathBuf>,
    /// The Adoptium API: [`ADOPTIUM_URL`].
    pub adoptium_url: String,
}

/// A runtime, and whether it has already been run to learn its version.
struct Candidate {
    runtime: Runtime,
    ran: bool,
}

impl Runtimes {
    /// This computer's locations: [`paths::java_runtimes_dir`], the
    /// environment, and the usual install folders for this OS.
    pub fn system() -> Result<Self, Error> {
        let var = |key| std::env::var_os(key).filter(|v| !v.is_empty());
        Ok(Self {
            managed: paths::java_runtimes_dir().ok_or(Error::NoRuntimesDir)?,
            java_home: var("JAVA_HOME").map(PathBuf::from),
            path: var("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            roots: detect::system_roots(),
            adoptium_url: ADOPTIUM_URL.to_owned(),
        })
    }

    /// Every runtime found, newest first, and by [`Source`] within a major
    /// version. Places that can't be read are skipped.
    pub async fn installed(&self) -> Vec<Runtime> {
        let mut runtimes: Vec<Runtime> = self
            .candidates()
            .await
            .into_iter()
            .map(|c| c.runtime)
            .collect();
        runtimes.sort_by_key(|r| (Reverse(r.major), r.source));
        runtimes
    }

    /// The Java to launch a version needing Java `required` with.
    ///
    /// `explicit` (see [`explicit_path`]; a Java home works too) wins if it
    /// runs and is Java `required` or newer; otherwise that's an error, not a
    /// silent substitute. Without it: an installed Java `required`, mctui's
    /// own first, or from Java 16 up the closest newer one; never an older
    /// one. The pick is run once to check it works. `Ok(None)` means nothing
    /// installed fits: [`Runtimes::install`] one.
    ///
    /// Don't save an automatic pick into the instance: pinned there, it would
    /// outlive both the runtime and the version's requirement.
    pub async fn find(
        &self,
        required: u32,
        explicit: Option<&Path>,
    ) -> Result<Option<Runtime>, Error> {
        if let Some(path) = explicit {
            return explicit_runtime(path, required).await.map(Some);
        }
        let mut candidates = self.candidates().await;
        candidates.retain(|c| fits(required, c.runtime.major));
        // Every candidate is `required` or newer, so the exact match sorts
        // first; the stable sort keeps discovery order among equals.
        candidates.sort_by_key(|c| (c.runtime.major, c.runtime.source));
        for Candidate { runtime, ran } in candidates {
            if ran || detect::probe(&runtime.java).await == Some(runtime.major) {
                return Ok(Some(runtime));
            }
            tracing::warn!(java = %runtime.java.display(), "skipping a Java that doesn't run");
        }
        Ok(None)
    }

    /// Downloads a Java `required` JRE from Adoptium, verifies its SHA-256,
    /// and unpacks it into `<managed>/<major>`, replacing what was there.
    /// Where Adoptium has no exact build it falls back: from Java 16 up to
    /// the next LTS (there is no Java 16 JRE), and on Apple silicon to an
    /// Intel build run by Rosetta (there is no arm64 Java 8).
    ///
    /// Reports the download's bytes, then the extracting phase. Cancel by
    /// dropping the future. A runtime is unpacked beside its final folder and
    /// renamed into place, so no half-unpacked runtime is ever left there; an
    /// unpack already under way finishes in the background.
    pub async fn install(
        &self,
        http: &Http,
        required: u32,
        mut on_progress: impl FnMut(InstallProgress),
    ) -> Result<Runtime, Error> {
        let package = adoptium::resolve(http, &self.adoptium_url, required).await?;
        let (major, total) = (package.major, package.size);
        tokio::fs::create_dir_all(&self.managed).await?;

        // Deleted once unpacked. One left by a crash still matches its
        // checksum, so the next attempt skips the download.
        let archive = self.managed.join(&package.name);
        let job = Download {
            url: package.url,
            dest: archive.clone(),
            hash: Some(Hash::Sha256(package.sha256)),
            size: Some(total),
        };
        let mut done = 0;
        on_progress(InstallProgress::Downloading { major, done, total });
        http.download(&job, |n| {
            done += n;
            on_progress(InstallProgress::Downloading { major, done, total });
        })
        .await?;

        on_progress(InstallProgress::Extracting { major });
        let managed = self.managed.clone();
        let java =
            tokio::task::spawn_blocking(move || unpack_into_place(&archive, &managed, major))
                .await
                .map_err(io::Error::other)??;
        Ok(Runtime {
            java,
            major,
            source: Source::Managed,
        })
    }

    /// Every runtime found, each with its major version: from its `release`
    /// file, or failing that by running it.
    async fn candidates(&self) -> Vec<Candidate> {
        let search = self.clone();
        let found = tokio::task::spawn_blocking(move || detect::scan(&search))
            .await
            .unwrap_or_default();
        let known = found.into_iter().map(|found| async move {
            let (major, ran) = match found.release_major {
                Some(major) => (major, false),
                None => (detect::probe(&found.java).await?, true),
            };
            let runtime = Runtime {
                java: found.java,
                major,
                source: found.source,
            };
            Some(Candidate { runtime, ran })
        });
        join_all(known).await.into_iter().flatten().collect()
    }
}

/// Checks the Java the user set: it must exist, run, and be new enough.
async fn explicit_runtime(path: &Path, required: u32) -> Result<Runtime, Error> {
    let broken = || Error::ExplicitBroken {
        path: path.to_owned(),
    };
    let java = match tokio::fs::metadata(path).await {
        Ok(meta) if meta.is_dir() => detect::java_in_home(path).await.ok_or_else(broken)?,
        Ok(_) => path.to_owned(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(Error::ExplicitMissing {
                path: path.to_owned(),
            });
        }
        Err(_) => return Err(broken()),
    };
    let found = detect::probe(&java).await.ok_or_else(broken)?;
    if found < required {
        return Err(Error::ExplicitTooOld {
            path: path.to_owned(),
            found,
            required,
        });
    }
    Ok(Runtime {
        java,
        major: found,
        source: Source::Explicit,
    })
}

/// Unpacks `archive` into a scratch folder in `managed`, checks it holds Java
/// `major`, and swaps it in as `<managed>/<major>`. Deletes the archive
/// either way. Returns the new runtime's executable. Blocking.
fn unpack_into_place(archive: &Path, managed: &Path, major: u32) -> Result<PathBuf, Error> {
    let result = unpack_checked(archive, managed, major);
    if let Err(e) = std::fs::remove_file(archive) {
        tracing::debug!(archive = %archive.display(), error = %e, "couldn't delete the Java archive");
    }
    result
}

fn unpack_checked(archive: &Path, managed: &Path, major: u32) -> Result<PathBuf, Error> {
    let staging = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(managed)?;
    extract::unpack(archive, staging.path())?;
    let java = detect::java_in(staging.path()).ok_or(Error::BadDownload { major })?;
    if detect::release_major(&java).is_some_and(|found| found != major) {
        return Err(Error::BadDownload { major });
    }
    let dest = managed.join(major.to_string());
    fsutil::replace_dir(staging.path(), &dest, managed)?;
    // `staging` now finds nothing to delete when dropped.
    detect::java_in(&dest).ok_or(Error::BadDownload { major })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_major_defaults_to_java_8() {
        assert_eq!(required_major(None), 8);
        assert_eq!(required_major(Some(0)), 8);
        assert_eq!(required_major(Some(21)), 21);
    }

    #[test]
    fn only_newer_versions_accept_a_newer_java() {
        for (required, major, want) in [
            (8, 8, true),
            (8, 17, false),
            (8, 7, false),
            (16, 16, true),
            (16, 17, true),
            (17, 17, true),
            (17, 21, true),
            (17, 16, false),
            (21, 17, false),
            (21, 25, true),
        ] {
            assert_eq!(fits(required, major), want, "Java {major} for {required}");
        }
    }

    #[test]
    fn the_instance_java_wins_over_the_app_one() {
        let mut instance = Instance::default();
        let mut config = Config::default();
        assert_eq!(explicit_path(&instance, &config), None);
        config.java_path = Some("/app/java".into());
        assert_eq!(
            explicit_path(&instance, &config),
            Some(Path::new("/app/java"))
        );
        instance.java_path = Some("/instance/java".into());
        assert_eq!(
            explicit_path(&instance, &config),
            Some(Path::new("/instance/java"))
        );
    }
}
