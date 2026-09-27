//! Java runtimes end to end: detection over a fake tree of runtimes, the
//! selection rules, and installs from a mock Adoptium.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mctui_core::java::{Error, InstallProgress, Runtime, Runtimes, Source};
use mctui_core::net::{self, Http};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const JAVA_EXE: &str = if cfg!(windows) { "java.exe" } else { "java" };

/// Searches only `<root>/managed` and the runtimes in `<root>/jvm`.
fn runtimes(root: &Path, adoptium_url: String) -> Runtimes {
    Runtimes {
        managed: root.join("managed"),
        java_home: None,
        path: Vec::new(),
        roots: vec![root.join("jvm")],
        adoptium_url,
    }
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
mod detection {
    use std::os::unix::fs::{PermissionsExt as _, symlink};

    use super::*;

    /// A runtime folder: a `bin/java` that prints `version` the way Java
    /// does (or fails, for `None`), and a `release` file if `release` is set.
    fn runtime(home: &Path, version: Option<&str>, release: Option<&str>) -> PathBuf {
        let java = home.join("bin/java");
        fs::create_dir_all(java.parent().unwrap()).unwrap();
        if let Some(release) = release {
            fs::write(
                home.join("release"),
                format!("JAVA_VERSION=\"{release}\"\n"),
            )
            .unwrap();
        }
        let script = match version {
            Some(v) => format!("#!/bin/sh\necho 'openjdk version \"{v}\" 2024-01-16' >&2\n"),
            None => "#!/bin/sh\nexit 1\n".to_owned(),
        };
        fs::write(&java, script).unwrap();
        fs::set_permissions(&java, fs::Permissions::from_mode(0o755)).unwrap();
        java
    }

    struct Tree {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        runtimes: Runtimes,
    }

    impl Tree {
        fn path(&self, rel: &str) -> PathBuf {
            self.root.join(rel)
        }

        fn at(&self, rel: &str, major: u32, source: Source) -> Runtime {
            Runtime {
                java: self.path(rel),
                major,
                source,
            }
        }
    }

    /// mctui's 21 (a macOS bundle) and 17; Java 8 in `$JAVA_HOME` and again
    /// through a symlink; an 11 without a `release` file, also on `$PATH`;
    /// and things that aren't runtimes.
    fn tree() -> Tree {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_owned();
        let at = |rel: &str| root.join(rel);

        runtime(
            &at("managed/21/Contents/Home"),
            Some("21.0.9"),
            Some("21.0.9"),
        );
        runtime(&at("managed/17"), Some("17.0.8"), Some("17.0.8"));
        runtime(&at("managed/.install-x1y2"), Some("25"), Some("25"));
        fs::write(at("managed/OpenJDK25U-jre.tar.gz"), "").unwrap();

        runtime(&at("jvm/temurin-8"), Some("1.8.0_392"), Some("1.8.0_392"));
        symlink(at("jvm/temurin-8"), at("jvm/zulu-8")).unwrap();
        runtime(&at("jvm/openjdk-11"), Some("11.0.2"), None);
        runtime(&at("jvm/broken"), None, None);
        fs::create_dir_all(at("jvm/not-java/bin")).unwrap();

        fs::create_dir(at("bin")).unwrap();
        symlink(at("jvm/openjdk-11/bin/java"), at("bin/java")).unwrap();

        let runtimes = Runtimes {
            java_home: Some(at("jvm/temurin-8")),
            path: vec![at("empty"), at("bin")],
            ..runtimes(&root, String::new())
        };
        Tree {
            _tmp: tmp,
            root,
            runtimes,
        }
    }

    #[tokio::test]
    async fn lists_each_runtime_once() {
        let tree = tree();
        assert_eq!(
            tree.runtimes.installed().await,
            [
                tree.at("managed/21/Contents/Home/bin/java", 21, Source::Managed),
                tree.at("managed/17/bin/java", 17, Source::Managed),
                tree.at("bin/java", 11, Source::Path),
                tree.at("jvm/temurin-8/bin/java", 8, Source::JavaHome),
            ]
        );
    }

    #[tokio::test]
    async fn picks_the_exact_major_else_the_closest_newer_one() {
        let tree = tree();
        let find = |required| tree.runtimes.find(required, None);
        let at = |rel, major, source| Some(tree.at(rel, major, source));

        let managed_21 = at("managed/21/Contents/Home/bin/java", 21, Source::Managed);
        assert_eq!(find(21).await.unwrap(), managed_21);
        let managed_17 = at("managed/17/bin/java", 17, Source::Managed);
        assert_eq!(find(17).await.unwrap(), managed_17);
        // Java 16 has no runtime here; 17 fits, 21 is further away.
        assert_eq!(find(16).await.unwrap(), managed_17);
        assert_eq!(find(11).await.unwrap(), at("bin/java", 11, Source::Path));
        let java_8 = at("jvm/temurin-8/bin/java", 8, Source::JavaHome);
        assert_eq!(find(8).await.unwrap(), java_8);
        // Nothing newer for 25, and never an older Java.
        assert_eq!(find(25).await.unwrap(), None);
    }

    #[tokio::test]
    async fn java_8_versions_never_get_a_newer_java() {
        let tmp = tempfile::tempdir().unwrap();
        runtime(
            &tmp.path().join("managed/17"),
            Some("17.0.8"),
            Some("17.0.8"),
        );
        let runtimes = runtimes(tmp.path(), String::new());
        assert_eq!(runtimes.find(8, None).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_runtime_that_does_not_run_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // Looks complete, as a half-unpacked download by the Go app could.
        runtime(&tmp.path().join("managed/21"), None, Some("21.0.9"));
        let good = runtime(&tmp.path().join("jvm/temurin-21"), Some("21.0.1"), None);
        let runtimes = runtimes(tmp.path(), String::new());

        let found = runtimes.find(21, None).await.unwrap().unwrap();
        assert_eq!((found.java, found.source), (good, Source::System));
    }

    #[tokio::test]
    async fn an_explicit_java_wins_when_new_enough() {
        let tree = tree();
        let runtimes = &tree.runtimes;
        let find = |required, explicit: PathBuf| async move {
            runtimes.find(required, Some(&explicit)).await
        };

        // Newer than needed is the user's call; a Java home works too.
        let found = find(8, tree.path("jvm/openjdk-11")).await.unwrap();
        let want = tree.at("jvm/openjdk-11/bin/java", 11, Source::Explicit);
        assert_eq!(found, Some(want));

        // Too old is an error, even though mctui has a Java 17.
        let err = find(17, tree.path("jvm/temurin-8/bin/java")).await;
        assert!(
            matches!(
                err,
                Err(Error::ExplicitTooOld {
                    found: 8,
                    required: 17,
                    ..
                })
            ),
            "{err:?}"
        );
        let err = find(17, tree.path("gone/bin/java")).await;
        assert!(matches!(err, Err(Error::ExplicitMissing { .. })), "{err:?}");
        let err = find(8, tree.path("jvm/broken/bin/java")).await;
        assert!(matches!(err, Err(Error::ExplicitBroken { .. })), "{err:?}");
        let err = find(8, tree.path("jvm/not-java")).await;
        assert!(matches!(err, Err(Error::ExplicitBroken { .. })), "{err:?}");
    }
}

mod install {
    use sha2::Digest as _;

    use super::*;

    /// A runtime archive as Adoptium ships it: one top-level folder holding
    /// `bin/java` and a `release` file.
    fn jre_tar_gz(major: u32, extra: &[(&str, &[u8])]) -> Vec<u8> {
        let java = format!("#!/bin/sh\necho 'openjdk version \"{major}.0.4\"' >&2\n");
        let release = format!("JAVA_VERSION=\"{major}.0.4\"\n");
        let top = format!("jdk-{major}.0.4+7-jre");
        let mut entries = vec![
            (format!("{top}/bin/{JAVA_EXE}"), java.into_bytes(), 0o755),
            (format!("{top}/release"), release.into_bytes(), 0o644),
            (format!("{top}/lib/modules"), vec![0; 4096], 0o644),
        ];
        for (name, data) in extra {
            entries.push(((*name).to_owned(), data.to_vec(), 0o644));
        }

        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for (name, data, mode) in entries {
            let mut header = tar::Header::new_gnu();
            // Written raw, so tests can include paths `set_path` refuses.
            header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_mode(mode);
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append(&header, data.as_slice()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    fn sha256(data: &[u8]) -> String {
        use std::fmt::Write as _;
        let digest = sha2::Sha256::digest(data);
        digest.iter().fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
    }

    /// Lists Java `major` on the mock Adoptium, in the shape of a real
    /// response, as an archive of `size` bytes published with `checksum`.
    async fn publish(server: &MockServer, major: u32, checksum: &str, size: usize) {
        let fixture = include_str!("fixtures/java/adoptium-21-linux-x64.json");
        let mut body: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let package = &mut body[0]["binaries"][0]["package"];
        package["name"] = format!("OpenJDK{major}U-jre_hotspot.tar.gz").into();
        package["link"] = format!("{}/download/{major}.tar.gz", server.uri()).into();
        package["checksum"] = checksum.into();
        package["size"] = size.into();
        Mock::given(method("GET"))
            .and(path(format!("/v3/assets/feature_releases/{major}/ga")))
            .and(query_param("image_type", "jre"))
            .and(query_param("jvm_impl", "hotspot"))
            .and(query_param("vendor", "eclipse"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    /// Serves the download of Java `major`.
    async fn serve(server: &MockServer, major: u32, download: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(format!("/download/{major}.tar.gz")))
            .respond_with(download)
            .mount(server)
            .await;
    }

    /// A mock Adoptium with `archive` as Java `major`, published with the
    /// archive's own checksum unless `checksum` says otherwise. Like the real
    /// one, it answers 404 for anything else.
    async fn adoptium(major: u32, archive: Vec<u8>, checksum: Option<&str>) -> MockServer {
        let server = MockServer::start().await;
        let checksum = checksum.map_or_else(|| sha256(&archive), str::to_owned);
        publish(&server, major, &checksum, archive.len()).await;
        let download = ResponseTemplate::new(200).set_body_bytes(archive);
        serve(&server, major, download).await;
        server
    }

    async fn adoptium_with(major: u32, archive: Vec<u8>) -> MockServer {
        adoptium(major, archive, None).await
    }

    #[tokio::test]
    async fn installs_a_verified_runtime_and_reports_progress() {
        let archive = jre_tar_gz(21, &[]);
        let size = archive.len() as u64;
        let server = adoptium_with(21, archive).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let mut events = Vec::new();
        let runtime = runtimes
            .install(&Http::new().unwrap(), 21, |p| events.push(p))
            .await
            .unwrap();

        let managed = tmp.path().join("managed");
        let want = Runtime {
            java: managed.join("21/bin").join(JAVA_EXE),
            major: 21,
            source: Source::Managed,
        };
        assert_eq!(runtime, want);
        assert_eq!(names_in(&managed), ["21"], "no archive or scratch left");
        assert_eq!(
            events.first(),
            Some(&InstallProgress::Downloading {
                major: 21,
                done: 0,
                total: size
            })
        );
        assert_eq!(
            events[events.len() - 2..],
            [
                InstallProgress::Downloading {
                    major: 21,
                    done: size,
                    total: size
                },
                InstallProgress::Extracting { major: 21 },
            ]
        );
        // What was installed is what's found next time.
        #[cfg(unix)]
        assert_eq!(runtimes.find(21, None).await.unwrap(), Some(want));
    }

    #[tokio::test]
    async fn a_bad_checksum_installs_nothing() {
        let wrong = sha256(b"something else");
        let server = adoptium(21, jre_tar_gz(21, &[]), Some(&wrong)).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let err = runtimes.install(&Http::new().unwrap(), 21, |_| {}).await;
        assert!(
            matches!(err, Err(Error::Net(net::Error::Checksum { .. }))),
            "{err:?}"
        );
        assert!(names_in(&tmp.path().join("managed")).is_empty());
    }

    #[tokio::test]
    async fn an_unsafe_archive_installs_nothing() {
        let evil: &[(&str, &[u8])] = &[("jdk-21.0.4+7-jre/../../evil", b"x")];
        let server = adoptium_with(21, jre_tar_gz(21, evil)).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let err = runtimes.install(&Http::new().unwrap(), 21, |_| {}).await;
        assert!(matches!(err, Err(Error::UnsafeArchive { .. })), "{err:?}");
        assert!(names_in(&tmp.path().join("managed")).is_empty());
        assert!(!tmp.path().join("evil").exists());
    }

    #[tokio::test]
    async fn the_wrong_java_in_the_archive_is_refused() {
        // Adoptium listing a Java 17 archive as Java 21.
        let server = adoptium_with(21, jre_tar_gz(17, &[])).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let err = runtimes.install(&Http::new().unwrap(), 21, |_| {}).await;
        assert!(
            matches!(err, Err(Error::BadDownload { major: 21 })),
            "{err:?}"
        );
        assert!(names_in(&tmp.path().join("managed")).is_empty());
    }

    #[tokio::test]
    async fn java_16_falls_back_to_the_next_lts() {
        let server = adoptium_with(17, jre_tar_gz(17, &[])).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let runtime = runtimes
            .install(&Http::new().unwrap(), 16, |_| {})
            .await
            .unwrap();
        assert_eq!(runtime.major, 17);
        assert!(runtime.java.starts_with(tmp.path().join("managed/17")));
    }

    #[tokio::test]
    async fn no_build_anywhere_is_unavailable() {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let err = runtimes.install(&Http::new().unwrap(), 8, |_| {}).await;
        assert!(
            matches!(err, Err(Error::Unavailable { major: 8 })),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn replaces_what_was_there() {
        let server = adoptium_with(21, jre_tar_gz(21, &[])).await;
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("managed/21");
        fs::create_dir_all(old.join("bin")).unwrap();
        fs::write(old.join("stale"), "from a broken install").unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        runtimes
            .install(&Http::new().unwrap(), 21, |_| {})
            .await
            .unwrap();
        assert_eq!(names_in(&old), ["bin", "lib", "release"]);
        assert_eq!(names_in(&tmp.path().join("managed")), ["21"]);
    }

    #[tokio::test]
    async fn cancelling_leaves_nothing_behind() {
        let server = MockServer::start().await;
        let archive = jre_tar_gz(21, &[]);
        publish(&server, 21, &sha256(&archive), archive.len()).await;
        let stalled = ResponseTemplate::new(200)
            .set_body_bytes(archive)
            .set_delay(Duration::from_secs(30));
        serve(&server, 21, stalled).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let http = Http::new().unwrap();
        let install = runtimes.install(&http, 21, |_| {});
        let cancelled = tokio::time::timeout(Duration::from_millis(500), install).await;
        assert!(cancelled.is_err(), "the download should still be waiting");
        assert!(names_in(&tmp.path().join("managed")).is_empty());
    }
}
