//! Java runtimes: find one that can run a Minecraft version, or install one
//! from Eclipse Adoptium.
//!
//! [`Runtimes::find`] picks the Java for what a version [`Needs`]: the one
//! set for the instance or in Settings if it can run it, else an installed
//! runtime of the exact major version (mctui's own first) or, from Java 16
//! up, the closest newer one. It never picks an older Java. When nothing
//! fits, [`Runtimes::install`] downloads a checksum-verified JRE into
//! mctui's runtimes folder.
//!
//! Each runtime's architecture is known too, since on Apple silicon it
//! matters: versions before 1.19 only have Intel native libraries for macOS,
//! so they need an Intel Java, which Rosetta runs. The game's libraries are
//! then chosen for the Java's architecture, not the computer's.

mod adoptium;
mod detect;
mod extract;
mod needs;

use std::cmp::Reverse;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use futures_util::future::join_all;

pub use adoptium::ADOPTIUM_URL;
pub use needs::Needs;

use crate::config::Config;
use crate::fsutil;
use crate::instance::Instance;
use crate::mojang::VersionJson;
use crate::mojang::rules::{Arch, Os};
use crate::net::{self, Download, Hash, Http};
use crate::paths::{self, DataDir};

/// The Java a version needs when its JSON doesn't say: before 1.17 the
/// version JSON had no `javaVersion`, and those versions run on Java 8.
pub const DEFAULT_MAJOR: u32 = 8;

/// Versions needing this Java or newer also run on anything newer. Older
/// ones (Java 8) often don't, so they only get an exact match.
const NEWER_OK_FROM: u32 = 16;

/// What can go wrong finding or installing Java.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The Java the user set doesn't exist.
    #[error("{origin} ({}) doesn't exist", path.display())]
    ExplicitMissing { origin: Origin, path: PathBuf },
    /// The Java the user set exists but doesn't run.
    #[error("{origin} ({}) doesn't run", path.display())]
    ExplicitBroken { origin: Origin, path: PathBuf },
    /// The Java the user set is older than the version needs.
    #[error(
        "{origin} ({}) is Java {found}, but this version needs Java {required}",
        path.display()
    )]
    ExplicitTooOld {
        origin: Origin,
        path: PathBuf,
        found: u32,
        required: u32,
    },
    /// The Java the user set is newer than the version runs on.
    #[error(
        "{origin} ({}) is Java {found}, but this version only runs on Java {newest}",
        path.display()
    )]
    ExplicitTooNew {
        origin: Origin,
        path: PathBuf,
        found: u32,
        newest: u32,
    },
    /// The Java the user set can't load the version's native libraries: an
    /// arm64 Java for a version with only Intel ones for macOS.
    #[error(
        "{origin} ({}) is an {} Java, but this version's native libraries need an {} one \
         (Rosetta runs it); clear it to let mctui choose",
        path.display(),
        found.as_str(),
        required.as_str()
    )]
    ExplicitWrongArch {
        origin: Origin,
        path: PathBuf,
        found: Arch,
        required: Arch,
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
    /// Adoptium offered a download that isn't a `.tar.gz` or `.zip`.
    #[error("the Java download ({name}) isn't a .tar.gz or .zip")]
    UnknownArchive { name: String },
    /// The download holds a path that would land outside its folder.
    #[error("the Java download has an unsafe path: {}", entry.display())]
    UnsafeArchive { entry: PathBuf },
    /// The download unpacked, but isn't the Java it should be: another
    /// version, or built for another architecture.
    #[error("the Java {major} download isn't the runtime it should be")]
    BadDownload { major: u32 },
    /// Unpacking or moving the runtime into place failed.
    #[error("couldn't install Java: {0}")]
    Io(#[from] io::Error),
}

/// Where the user set a Java. Displays as the start of a sentence about it,
/// e.g. "this instance's Java".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The instance's own.
    Instance,
    /// The app-wide one in Settings.
    Settings,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Instance => "this instance's Java",
            Self::Settings => "the Java set in Settings",
        })
    }
}

/// An installed Java.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    /// The executable to launch (`java`, or `java.exe` on Windows).
    pub java: PathBuf,
    /// The major version: 8, 17, 21, ….
    pub major: u32,
    /// The architecture it's built for, from its `release` file; this
    /// computer's when it doesn't say.
    pub arch: Arch,
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

/// The Javas the user set, which [`Runtimes::find`] tries before any other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Choice<'a> {
    /// The instance's own. The Go app saved its automatic picks here too, so
    /// one that looks like a stale pick is set aside: see [`StaleJava`].
    pub instance: Option<&'a Path>,
    /// The app-wide one in Settings, for instances without their own.
    pub settings: Option<&'a Path>,
}

impl<'a> Choice<'a> {
    /// The Javas set for `instance` and in `config`.
    pub fn of(instance: &'a Instance, config: &'a Config) -> Self {
        Self {
            instance: instance.java_path.as_deref(),
            settings: config.java_path.as_deref(),
        }
    }
}

/// What [`Runtimes::find`] picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    /// The Java to launch with. `None` when the user set none and nothing
    /// installed fits: [`Runtimes::install`] one.
    pub runtime: Option<Runtime>,
    /// The instance's own Java, if it was set aside as stale.
    pub stale: Option<StaleJava>,
}

/// The instance's Java, set aside by [`Runtimes::find`] as a stale pick
/// rather than the user's choice.
///
/// The Go app saved the Java it picked into every instance it launched. When
/// that Java has since gone (a Homebrew upgrade removes the old one), is too
/// old for the instance's version now, or is one of the runtimes mctui
/// manages (and replaces), it's no choice to hold the user to. The instance
/// should forget it, and the user hear why: it displays as a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleJava {
    /// The instance's Java.
    pub path: PathBuf,
    /// Why it was set aside.
    pub why: Staleness,
}

/// Why the instance's Java was set aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staleness {
    /// It doesn't exist anymore.
    Missing,
    /// It's in a managed runtimes folder, mctui's or the Go app's, so it was
    /// an automatic pick.
    Managed,
    /// It's older than the version needs.
    TooOld { found: u32, required: u32 },
}

impl fmt::Display for StaleJava {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.display();
        write!(f, "This instance's Java ({path}) ")?;
        match self.why {
            Staleness::Missing => f.write_str("no longer exists"),
            Staleness::Managed => f.write_str("was picked automatically by an older mctui"),
            Staleness::TooOld { found, required } => {
                write!(f, "is Java {found}, but this version needs Java {required}")
            }
        }?;
        f.write_str(", so it was cleared and Java is chosen as usual.")
    }
}

/// The Java major version a Minecraft version needs, from its JSON's
/// `javaVersion.majorVersion`; [`DEFAULT_MAJOR`] when that's absent.
pub fn required_major(declared: Option<u32>) -> u32 {
    declared.filter(|&major| major > 0).unwrap_or(DEFAULT_MAJOR)
}

/// Whether an automatic pick of Java `major` can run a version needing
/// `required`.
fn fits(required: u32, major: u32) -> bool {
    major == required || (required >= NEWER_OK_FROM && major > required)
}

/// Where runtimes are looked for and installed, the Adoptium API, and the
/// computer they're for. [`Runtimes::system`] fills it in for this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtimes {
    /// mctui's own runtimes, one folder per major version (`<managed>/21`),
    /// and architecture when it isn't this computer's (`<managed>/8-x86_64`).
    pub managed: PathBuf,
    /// Where the Go app installed runtimes, if known: the same folder as
    /// `managed` except in portable mode. Not searched; a Java pinned from
    /// it was one of the Go app's automatic picks.
    pub go_managed: Option<PathBuf>,
    /// `$JAVA_HOME`.
    pub java_home: Option<PathBuf>,
    /// The folders on `$PATH`. Relative ones are skipped.
    pub path: Vec<PathBuf>,
    /// Folders whose subfolders are runtimes, like `/usr/lib/jvm`.
    pub roots: Vec<PathBuf>,
    /// The Adoptium API: [`ADOPTIUM_URL`].
    pub adoptium_url: String,
    /// The computer's OS: [`Os::current`].
    pub os: Os,
    /// The computer's architecture: [`Arch::current`]. Runtimes that don't
    /// say what they're built for are taken to be built for it.
    pub arch: Arch,
}

/// A runtime, and whether it has already been run to learn its version.
struct Candidate {
    runtime: Runtime,
    ran: bool,
}

impl Runtimes {
    /// This computer's locations: [`paths::java_runtimes_dir`] for `data`,
    /// the environment, and the usual install folders for this OS.
    pub fn system(data: &DataDir) -> Result<Self, Error> {
        let var = |key| std::env::var_os(key).filter(|v| !v.is_empty());
        Ok(Self {
            managed: paths::java_runtimes_dir(data).ok_or(Error::NoRuntimesDir)?,
            go_managed: paths::go_java_runtimes_dir(),
            java_home: var("JAVA_HOME").map(PathBuf::from),
            path: var("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            roots: detect::system_roots(),
            adoptium_url: ADOPTIUM_URL.to_owned(),
            os: Os::current(),
            arch: Arch::current(),
        })
    }

    /// What `version` needs of Java on this computer.
    pub fn needs(&self, version: &VersionJson) -> Needs {
        Needs::of(version, self.os, self.arch)
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

    /// The Java to launch a version with `needs`.
    ///
    /// A Java the user set (see [`Choice`]; a Java home works too) wins if
    /// it can run the version; otherwise that's an error, not a silent
    /// substitute. The instance's own is set aside instead ([`Pick::stale`])
    /// when it looks like a stale pick the Go app saved: missing, in a
    /// managed runtimes folder, or too old. Then the search goes on as if it
    /// weren't set.
    ///
    /// Without one: an installed Java `needs.major`, mctui's own first, or
    /// from Java 16 up the closest newer one; never an older one. It must be
    /// of the architecture `needs` asks for, if any, and this computer's
    /// comes first. The pick is run once to check it works.
    ///
    /// Don't save an automatic pick into the instance: pinned there, it would
    /// outlive both the runtime and the version's requirement.
    pub async fn find(&self, needs: &Needs, choice: Choice<'_>) -> Result<Pick, Error> {
        let mut stale = None;
        if let Some(path) = choice.instance {
            let why = if self.is_managed(path) {
                Staleness::Managed
            } else {
                match explicit_runtime(path, Origin::Instance, needs, self.arch).await {
                    Ok(runtime) => {
                        return Ok(Pick {
                            runtime: Some(runtime),
                            stale: None,
                        });
                    }
                    Err(Error::ExplicitMissing { .. }) => Staleness::Missing,
                    Err(Error::ExplicitTooOld {
                        found, required, ..
                    }) => Staleness::TooOld { found, required },
                    Err(e) => return Err(e),
                }
            };
            tracing::info!(java = %path.display(), ?why, "setting aside the instance's stale Java");
            stale = Some(StaleJava {
                path: path.to_owned(),
                why,
            });
        }
        let runtime = match choice.settings {
            Some(path) => Some(explicit_runtime(path, Origin::Settings, needs, self.arch).await?),
            None => self.automatic(needs).await,
        };
        Ok(Pick { runtime, stale })
    }

    /// Downloads a JRE for `needs` from Adoptium, verifies its SHA-256, and
    /// unpacks it into `managed` (see [`Runtimes::managed`]), replacing what
    /// was there. Where Adoptium has no exact build it falls back: from Java
    /// 16 up to the next LTS (there is no Java 16 JRE), and on Apple silicon
    /// to an Intel build run by Rosetta (there is no arm64 Java 8). Where
    /// `needs` asks for an architecture, it gets that one.
    ///
    /// Reports the download's bytes, then the extracting phase. Cancel by
    /// dropping the future. A runtime is unpacked beside its final folder and
    /// renamed into place, so no half-unpacked runtime is ever left there; an
    /// unpack already under way finishes in the background.
    pub async fn install(
        &self,
        http: &Http,
        needs: &Needs,
        mut on_progress: impl FnMut(InstallProgress),
    ) -> Result<Runtime, Error> {
        let package =
            adoptium::resolve(http, &self.adoptium_url, needs, self.os, self.arch).await?;
        let (major, arch, total) = (package.major, package.arch, package.size);
        tokio::fs::create_dir_all(&self.managed).await?;

        // Named here: Adoptium's name for it comes off the network, and could
        // point anywhere. Deleted once unpacked. One left by a crash still
        // matches its checksum, so the next attempt skips the download.
        let name = format!("{major}-{}{}", arch.as_str(), package.archive.extension());
        let archive = self.managed.join(name);
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
        let dest = self.managed.join(self.folder(major, arch));
        let unpack = move || unpack_into_place(&archive, &managed, &dest, major, arch);
        let java = tokio::task::spawn_blocking(unpack)
            .await
            .map_err(io::Error::other)??;
        Ok(Runtime {
            java,
            major,
            arch,
            source: Source::Managed,
        })
    }

    /// The best installed runtime for `needs` that runs, if any.
    async fn automatic(&self, needs: &Needs) -> Option<Runtime> {
        let wanted = needs.arch.unwrap_or(self.arch);
        let mut candidates = self.candidates().await;
        candidates.retain(|c| {
            needs.fits(c.runtime.major) && needs.arch.is_none_or(|arch| c.runtime.arch == arch)
        });
        // This computer's architecture first: another one runs emulated, if
        // at all. Every candidate is `needs.major` or newer, so the exact
        // match sorts next; the stable sort keeps discovery order among
        // equals.
        candidates.sort_by_key(|c| (c.runtime.arch != wanted, c.runtime.major, c.runtime.source));
        for Candidate { runtime, ran } in candidates {
            if ran || detect::probe(&runtime.java).await == Some(runtime.major) {
                return Some(runtime);
            }
            tracing::warn!(java = %runtime.java.display(), "skipping a Java that doesn't run");
        }
        None
    }

    /// Every runtime found, each with its major version (from its `release`
    /// file, or failing that by running it) and architecture.
    async fn candidates(&self) -> Vec<Candidate> {
        let search = self.clone();
        let found = tokio::task::spawn_blocking(move || detect::scan(&search))
            .await
            .unwrap_or_default();
        let host = self.arch;
        let known = found.into_iter().map(|found| async move {
            let (major, ran) = match found.release.major {
                Some(major) => (major, false),
                None => (detect::probe(&found.java).await?, true),
            };
            let runtime = Runtime {
                java: found.java,
                major,
                arch: found.release.arch.unwrap_or(host),
                source: found.source,
            };
            Some(Candidate { runtime, ran })
        });
        join_all(known).await.into_iter().flatten().collect()
    }

    /// Whether `java` is in a managed runtimes folder, mctui's or the Go
    /// app's.
    fn is_managed(&self, java: &Path) -> bool {
        std::iter::once(&self.managed)
            .chain(&self.go_managed)
            .any(|dir| java.starts_with(dir))
    }

    /// The folder in `managed` for Java `major` built for `arch`: `<major>`,
    /// or `<major>-<arch>` when that isn't this computer's, so an Intel Java
    /// for old versions doesn't replace the native one of the same version.
    fn folder(&self, major: u32, arch: Arch) -> String {
        if arch == self.arch {
            major.to_string()
        } else {
            format!("{major}-{}", arch.as_str())
        }
    }
}

/// Checks a Java the user set: it must exist, run, be a version that can
/// run the game, and be built for the architecture `needs` asks for, if
/// any. One whose `release` file doesn't say is taken to be built for
/// `host`.
async fn explicit_runtime(
    path: &Path,
    origin: Origin,
    needs: &Needs,
    host: Arch,
) -> Result<Runtime, Error> {
    let broken = || Error::ExplicitBroken {
        origin,
        path: path.to_owned(),
    };
    let java = match tokio::fs::metadata(path).await {
        Ok(meta) if meta.is_dir() => detect::java_in_home(path).await.ok_or_else(broken)?,
        Ok(_) => path.to_owned(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(Error::ExplicitMissing {
                origin,
                path: path.to_owned(),
            });
        }
        Err(_) => return Err(broken()),
    };
    let found = detect::probe(&java).await.ok_or_else(broken)?;
    if found < needs.major {
        return Err(Error::ExplicitTooOld {
            origin,
            path: path.to_owned(),
            found,
            required: needs.major,
        });
    }
    if let Some(newest) = needs.newest
        && found > newest
    {
        return Err(Error::ExplicitTooNew {
            origin,
            path: path.to_owned(),
            found,
            newest,
        });
    }
    let arch = detect::release_of(&java).await.arch.unwrap_or(host);
    if let Some(required) = needs.arch
        && arch != required
    {
        return Err(Error::ExplicitWrongArch {
            origin,
            path: path.to_owned(),
            found: arch,
            required,
        });
    }
    Ok(Runtime {
        java,
        major: found,
        arch,
        source: Source::Explicit,
    })
}

/// Unpacks `archive` into a scratch folder in `managed`, checks it holds Java
/// `major` built for `arch`, and swaps it in as `dest`. Deletes the archive
/// either way. Returns the new runtime's executable. Blocking.
fn unpack_into_place(
    archive: &Path,
    managed: &Path,
    dest: &Path,
    major: u32,
    arch: Arch,
) -> Result<PathBuf, Error> {
    let result = unpack_checked(archive, managed, dest, major, arch);
    if let Err(e) = std::fs::remove_file(archive) {
        tracing::debug!(archive = %archive.display(), error = %e, "couldn't delete the Java archive");
    }
    result
}

fn unpack_checked(
    archive: &Path,
    managed: &Path,
    dest: &Path,
    major: u32,
    arch: Arch,
) -> Result<PathBuf, Error> {
    let staging = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(managed)?;
    extract::unpack(archive, staging.path())?;
    let java = detect::java_in(staging.path()).ok_or(Error::BadDownload { major })?;
    // What its `release` file says, if anything, must be what was asked for.
    let release = detect::release(&java);
    let other_major = release.major.is_some_and(|found| found != major);
    let other_arch = release.arch.is_some_and(|found| found != arch);
    if other_major || other_arch {
        return Err(Error::BadDownload { major });
    }
    fsutil::replace_dir(staging.path(), dest, managed)?;
    // `staging` now finds nothing to delete when dropped.
    detect::java_in(dest).ok_or(Error::BadDownload { major })
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
    fn the_instance_java_and_the_settings_one() {
        let mut instance = Instance::default();
        let mut config = Config::default();
        assert_eq!(Choice::of(&instance, &config), Choice::default());
        config.java_path = Some("/app/java".into());
        instance.java_path = Some("/instance/java".into());
        assert_eq!(
            Choice::of(&instance, &config),
            Choice {
                instance: Some(Path::new("/instance/java")),
                settings: Some(Path::new("/app/java")),
            }
        );
    }

    #[test]
    fn messages_say_where_the_java_was_set() {
        let missing = |origin| Error::ExplicitMissing {
            origin,
            path: "/gone/java".into(),
        };
        assert_eq!(
            missing(Origin::Instance).to_string(),
            "this instance's Java (/gone/java) doesn't exist"
        );
        assert_eq!(
            missing(Origin::Settings).to_string(),
            "the Java set in Settings (/gone/java) doesn't exist"
        );
        let wrong_arch = Error::ExplicitWrongArch {
            origin: Origin::Instance,
            path: "/opt/homebrew/bin/java".into(),
            found: Arch::Arm64,
            required: Arch::X86_64,
        };
        assert_eq!(
            wrong_arch.to_string(),
            "this instance's Java (/opt/homebrew/bin/java) is an arm64 Java, but this \
             version's native libraries need an x86_64 one (Rosetta runs it); clear it to \
             let mctui choose"
        );

        let stale = |why| {
            StaleJava {
                path: "/old/java".into(),
                why,
            }
            .to_string()
        };
        assert_eq!(
            stale(Staleness::Missing),
            "This instance's Java (/old/java) no longer exists, so it was cleared and Java \
             is chosen as usual."
        );
        assert_eq!(
            stale(Staleness::TooOld {
                found: 17,
                required: 21
            }),
            "This instance's Java (/old/java) is Java 17, but this version needs Java 21, so \
             it was cleared and Java is chosen as usual."
        );
    }

    fn runtimes() -> Runtimes {
        Runtimes {
            managed: "/config/mctui/java".into(),
            go_managed: Some("/go-config/mctui/java".into()),
            java_home: None,
            path: Vec::new(),
            roots: Vec::new(),
            adoptium_url: String::new(),
            os: Os::MacOs,
            arch: Arch::Arm64,
        }
    }

    #[test]
    fn managed_folders_are_mctuis_and_the_go_apps() {
        let runtimes = runtimes();
        for (java, want) in [
            ("/config/mctui/java/21/bin/java", true),
            ("/go-config/mctui/java/17/Contents/Home/bin/java", true),
            ("/config/mctui/javanese/bin/java", false),
            ("/opt/homebrew/opt/openjdk/bin/java", false),
        ] {
            assert_eq!(runtimes.is_managed(Path::new(java)), want, "{java}");
        }
    }

    #[test]
    fn another_architecture_gets_its_own_folder() {
        let runtimes = runtimes();
        assert_eq!(runtimes.folder(21, Arch::Arm64), "21");
        assert_eq!(runtimes.folder(8, Arch::X86_64), "8-x86_64");
    }
}
