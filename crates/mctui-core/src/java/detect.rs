//! Finding installed runtimes, and learning their major version and
//! architecture cheaply: from the `release` file every modern runtime ships,
//! else by running `java -version` (which doesn't tell the architecture).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use super::{Runtimes, Source};
use crate::mojang::rules::Arch;

/// How long `java -version` may take: a cold JVM on a slow disk can take a
/// few seconds.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The executable's file name.
const JAVA_EXE: &str = if cfg!(windows) { "java.exe" } else { "java" };

/// A runtime on disk.
pub(super) struct Found {
    /// The executable, as found (maybe through a symlink).
    pub java: PathBuf,
    pub source: Source,
    /// What its `release` file says.
    pub release: Release,
}

/// What a runtime's `release` file says about it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Release {
    /// From `JAVA_VERSION`.
    pub major: Option<u32>,
    /// From `OS_ARCH`.
    pub arch: Option<Arch>,
}

/// Every runtime in `search`'s locations, in [`Source`] order, each once:
/// runtimes reached by different paths are told apart by where their
/// executable really is. Blocking.
pub(super) fn scan(search: &Runtimes) -> Vec<Found> {
    let managed = subdirs(&search.managed)
        .into_iter()
        .filter(|home| is_major_folder(home))
        .filter_map(|home| java_in(&home))
        .map(|java| (java, Source::Managed));
    let java_home = search
        .java_home
        .iter()
        .filter_map(|home| java_in(home))
        .map(|java| (java, Source::JavaHome));
    // An empty or relative `$PATH` entry means the current folder, and a
    // `java` there is whatever happens to be in it.
    let path = search
        .path
        .iter()
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(JAVA_EXE))
        .filter(|java| java.is_file())
        .map(|java| (java, Source::Path));
    let system = search
        .roots
        .iter()
        .flat_map(|root| subdirs(root))
        .filter_map(|home| java_in(&home))
        .map(|java| (java, Source::System));

    let mut seen = HashSet::new();
    managed
        .chain(java_home)
        .chain(path)
        .chain(system)
        .filter_map(|(java, source)| {
            let real = fs::canonicalize(&java).ok()?;
            // macOS's /usr/bin/java only forwards to a runtime found elsewhere.
            let stub = cfg!(target_os = "macos") && real == Path::new("/usr/bin/java");
            let new = !stub && seen.insert(real.clone());
            new.then(|| Found {
                release: release(&real),
                java,
                source,
            })
        })
        .collect()
}

/// Where the executable would be in a runtime folder: `bin/java`, or in a
/// macOS bundle, `Contents/Home/bin/java`.
fn layouts(home: &Path) -> [PathBuf; 2] {
    [
        home.join("bin").join(JAVA_EXE),
        home.join("Contents/Home/bin").join(JAVA_EXE),
    ]
}

/// The executable in the runtime folder `home`. Blocking.
pub(super) fn java_in(home: &Path) -> Option<PathBuf> {
    layouts(home).into_iter().find(|java| java.is_file())
}

/// [`java_in`], without blocking.
pub(super) async fn java_in_home(home: &Path) -> Option<PathBuf> {
    for java in layouts(home) {
        if tokio::fs::metadata(&java).await.is_ok_and(|m| m.is_file()) {
            return Some(java);
        }
    }
    None
}

/// mctui keeps runtimes in folders named for their major version, plus the
/// architecture when it isn't this computer's: `21`, `8-x86_64`. Anything else
/// in there (a download, an install under way) isn't one.
fn is_major_folder(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let major = name.split_once('-').map_or(name, |(major, _)| major);
            major.parse::<u32>().is_ok()
        })
}

/// The folders in `dir`, symlinks followed, sorted by name; none if it can't
/// be read.
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// The `release` file of the runtime whose executable is `java` (its real
/// path: the file sits beside `bin/`); empty if it has none. Blocking.
pub(super) fn release(java: &Path) -> Release {
    let Some(home) = java.parent().and_then(Path::parent) else {
        return Release::default();
    };
    fs::read_to_string(home.join("release"))
        .map(|text| parse_release(&text))
        .unwrap_or_default()
}

/// [`release`] for an executable that may be a symlink, without blocking.
pub(super) async fn release_of(java: &Path) -> Release {
    match tokio::fs::canonicalize(java).await {
        Ok(real) => tokio::task::spawn_blocking(move || release(&real))
            .await
            .unwrap_or_default(),
        Err(_) => Release::default(),
    }
}

/// The major version and architecture in a `release` file's
/// `JAVA_VERSION="17.0.8"` and `OS_ARCH="aarch64"` lines.
fn parse_release(text: &str) -> Release {
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key)?.strip_prefix('='))
            .map(|value| value.trim().trim_matches('"'))
    };
    Release {
        major: value("JAVA_VERSION").and_then(parse_major),
        arch: value("OS_ARCH").and_then(parse_arch),
    }
}

/// An architecture as Java names it (`os.arch`, or `OS_ARCH` in `release`):
/// `amd64` and `x86_64` are the same, as are `aarch64` and `arm64`. `None`
/// for the rest, which rules don't tell apart.
fn parse_arch(name: &str) -> Option<Arch> {
    match name {
        "x86_64" | "amd64" | "x64" => Some(Arch::X86_64),
        "aarch64" | "arm64" => Some(Arch::Arm64),
        "x86" | "i386" | "i486" | "i586" | "i686" => Some(Arch::X86),
        _ => None,
    }
}

/// Runs `java -version` to learn its major version. `None` if it doesn't
/// start, fails, or hangs (then it's killed).
pub(super) async fn probe(java: &Path) -> Option<u32> {
    let run = tokio::process::Command::new(java)
        .arg("-version")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(PROBE_TIMEOUT, run).await.ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    // Java prints its version to stderr; some builds use stdout.
    [output.stderr, output.stdout]
        .iter()
        .find_map(|out| parse_version_output(&String::from_utf8_lossy(out)))
}

/// The major version in `java -version` output: from the first
/// `version "…"`, since lines like "Picked up `JAVA_TOOL_OPTIONS`" can come
/// before it.
fn parse_version_output(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        let (_, rest) = line.split_once("version \"")?;
        let (version, _) = rest.split_once('"')?;
        parse_major(version)
    })
}

/// The major version in a Java version string: `1.8.0_392` is 8, `17.0.8`
/// is 17, `21-ea` is 21.
fn parse_major(version: &str) -> Option<u32> {
    let mut numbers = version.split(|c: char| !c.is_ascii_digit());
    let major = match numbers.next()?.parse().ok()? {
        // Java 8 and older called themselves 1.x.
        1 => numbers.next()?.parse().ok()?,
        major => major,
    };
    (major > 0).then_some(major)
}

/// The folders on this OS whose subfolders are runtimes.
pub(super) fn system_roots() -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let in_home = |rel: &str| home.as_ref().map(|home| home.join(rel));
    let mut roots = Vec::new();
    if cfg!(target_os = "macos") {
        // Homebrew's `opt` holds every formula; only JDKs have a `bin/java`.
        let dirs = [
            "/Library/Java/JavaVirtualMachines",
            "/opt/homebrew/opt",
            "/usr/local/opt",
        ];
        roots.extend(dirs.map(PathBuf::from));
        roots.extend(in_home("Library/Java/JavaVirtualMachines"));
    } else if cfg!(windows) {
        let program_files = std::env::var_os("ProgramFiles")
            .map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from);
        let vendors = ["Java", "Eclipse Adoptium", "Microsoft", "Zulu"];
        roots.extend(vendors.map(|vendor| program_files.join(vendor)));
    } else {
        roots.extend(["/usr/lib/jvm", "/usr/lib64/jvm", "/usr/java"].map(PathBuf::from));
    }
    roots.extend(in_home(".sdkman/candidates/java"));
    roots.extend(in_home(".jenv/versions"));
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_from_version_strings() {
        for (version, want) in [
            ("1.8.0_392", Some(8)),
            ("1.8.0", Some(8)),
            ("1.7.0_80", Some(7)),
            ("9", Some(9)),
            ("11.0.21", Some(11)),
            ("17.0.8", Some(17)),
            ("17.0.8.1", Some(17)),
            ("21", Some(21)),
            ("21-ea", Some(21)),
            ("25.0.1+8-LTS", Some(25)),
            ("1", None),
            ("0.1", None),
            ("", None),
            ("abc", None),
        ] {
            assert_eq!(parse_major(version), want, "{version:?}");
        }
    }

    #[test]
    fn major_from_java_version_output() {
        let temurin = "openjdk version \"17.0.9\" 2023-10-17\n\
                       OpenJDK Runtime Environment Temurin-17.0.9+9 (build 17.0.9+9)\n\
                       OpenJDK 64-Bit Server VM Temurin-17.0.9+9 (build 17.0.9+9, mixed mode)";
        let oracle_8 = "java version \"1.8.0_391\"\n\
                        Java(TM) SE Runtime Environment (build 1.8.0_391-b13)";
        let picked_up = "Picked up JAVA_TOOL_OPTIONS: -Dfile.encoding=UTF-8\n\
                         openjdk version \"21-ea\" 2023-09-19";
        let macos_stub = "The operation couldn't be completed. Unable to locate a Java Runtime.";
        for (output, want) in [
            (temurin, Some(17)),
            (oracle_8, Some(8)),
            (picked_up, Some(21)),
            (macos_stub, None),
            ("", None),
        ] {
            assert_eq!(parse_version_output(output), want, "{output:?}");
        }
    }

    #[test]
    fn major_from_release_files() {
        let temurin = "IMPLEMENTOR=\"Eclipse Adoptium\"\n\
                       JAVA_RUNTIME_VERSION=\"21.0.9+10-LTS\"\n\
                       JAVA_VERSION=\"21.0.9\"\n\
                       JAVA_VERSION_DATE=\"2025-10-21\"\n";
        for (text, want) in [
            (temurin, Some(21)),
            ("JAVA_VERSION=\"1.8.0_392\"\n", Some(8)),
            (
                "JAVA_VERSION=\"17.0.8\"\r\nOS_NAME=\"Windows\"\r\n",
                Some(17),
            ),
            ("JAVA_VERSION=11.0.2\n", Some(11)),
            ("IMPLEMENTOR=\"Oracle\"\n", None),
            ("JAVA_VERSION=\"\"\n", None),
            // Not a key that merely starts the same.
            ("JAVA_VERSION_DATE=\"2025-10-21\"\n", None),
        ] {
            assert_eq!(parse_release(text).major, want, "{text:?}");
        }
    }

    #[test]
    fn architecture_from_release_files() {
        let temurin_mac = "IMPLEMENTOR=\"Eclipse Adoptium\"\n\
                           JAVA_VERSION=\"17.0.12\"\n\
                           OS_ARCH=\"aarch64\"\n\
                           OS_NAME=\"Darwin\"\n";
        for (text, want) in [
            (temurin_mac, Some(Arch::Arm64)),
            ("OS_ARCH=\"x86_64\"\n", Some(Arch::X86_64)),
            // Java 8 on Linux and Windows.
            ("OS_ARCH=\"amd64\"\r\n", Some(Arch::X86_64)),
            ("OS_ARCH=\"i586\"\n", Some(Arch::X86)),
            ("OS_ARCH=\"ppc64le\"\n", None),
            ("JAVA_VERSION=\"17\"\n", None),
        ] {
            assert_eq!(parse_release(text).arch, want, "{text:?}");
        }
        assert_eq!(
            parse_release(temurin_mac),
            Release {
                major: Some(17),
                arch: Some(Arch::Arm64)
            }
        );
    }

    #[test]
    fn finds_both_executable_layouts() {
        let root = tempfile::tempdir().unwrap();
        let (plain, bundle) = (root.path().join("plain"), root.path().join("bundle.jdk"));
        let [plain_java, _] = layouts(&plain);
        let [_, bundle_java] = layouts(&bundle);
        for java in [&plain_java, &bundle_java] {
            fs::create_dir_all(java.parent().unwrap()).unwrap();
            fs::write(java, "").unwrap();
        }
        assert_eq!(java_in(&plain), Some(plain_java));
        assert_eq!(java_in(&bundle), Some(bundle_java));
        assert_eq!(java_in(root.path()), None);
    }
}
