//! Java runtimes end to end: detection over a fake tree of runtimes, the
//! selection rules (architectures, and the instance's stale Java, included),
//! and installs from a mock Adoptium.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mctui_core::java::{Choice, Error, InstallProgress, Needs, Runtime, Runtimes, Source};
use mctui_core::mojang::rules::{Arch, Os};
use mctui_core::mojang::{JavaVersion, VersionJson};
use mctui_core::net::{self, Http};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const JAVA_EXE: &str = if cfg!(windows) { "java.exe" } else { "java" };

/// Apple silicon.
const MAC_ARM: (Os, Arch) = (Os::MacOs, Arch::Arm64);

/// Searches only `<root>/managed` and the runtimes in `<root>/jvm`, on this
/// computer.
fn runtimes(root: &Path, adoptium_url: String) -> Runtimes {
    Runtimes {
        managed: root.join("managed"),
        go_managed: None,
        java_home: None,
        path: Vec::new(),
        roots: vec![root.join("jvm")],
        adoptium_url,
        os: Os::current(),
        arch: Arch::current(),
    }
}

/// [`runtimes`], on another computer.
fn runtimes_on(root: &Path, adoptium_url: String, (os, arch): (Os, Arch)) -> Runtimes {
    Runtimes {
        os,
        arch,
        ..runtimes(root, adoptium_url)
    }
}

fn version(name: &str) -> VersionJson {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mojang")
        .join(name);
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

/// A version needing `major` with 1.16.5's libraries, as 1.17 to 1.18.2 are:
/// Java 16 or 17, but still LWJGL 3.2.1 on macOS.
fn old_natives_on_java(major: u32) -> VersionJson {
    VersionJson {
        java_version: Some(JavaVersion {
            component: "java-runtime-gamma".into(),
            major_version: major,
        }),
        ..version("1.16.5.json")
    }
}

/// A version needing `major` with 1.21.8's libraries, as 1.19 and on have:
/// LWJGL 3.3 with arm64 natives for macOS.
fn new_natives_on_java(major: u32) -> VersionJson {
    VersionJson {
        java_version: Some(JavaVersion {
            component: "java-runtime-gamma".into(),
            major_version: major,
        }),
        ..version("1.21.8.json")
    }
}

/// What `runtimes` picks by itself for `needs`.
async fn auto(runtimes: &Runtimes, needs: &Needs) -> Option<Runtime> {
    let pick = runtimes.find(needs, Choice::default()).await.unwrap();
    assert_eq!(pick.stale, None);
    pick.runtime
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
    use std::path::Component;

    use mctui_core::java::{Origin, Pick, StaleJava, Staleness};

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

    /// A runtime of Java `version` whose `release` file says it's built for
    /// `os_arch`, as Java names architectures.
    fn runtime_for(home: &Path, version: &str, os_arch: &str) -> PathBuf {
        let java = runtime(home, Some(version), None);
        let release = format!("JAVA_VERSION=\"{version}\"\nOS_ARCH=\"{os_arch}\"\n");
        fs::write(home.join("release"), release).unwrap();
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

        /// The runtime at `rel`, of this computer's architecture.
        fn at(&self, rel: &str, major: u32, source: Source) -> Runtime {
            self.built_for(rel, major, Arch::current(), source)
        }

        fn built_for(&self, rel: &str, major: u32, arch: Arch, source: Source) -> Runtime {
            Runtime {
                java: self.path(rel),
                major,
                arch,
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

    /// Runtimes on Apple silicon: Java 8, 17, and 21 for arm64, and 8 and
    /// 17 for Intel as well.
    fn mac_tree() -> Tree {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_owned();
        let at = |rel: &str| root.join(rel);
        runtime_for(&at("jvm/zulu-8"), "1.8.0_462", "aarch64");
        runtime_for(&at("jvm/temurin-8"), "1.8.0_462", "x86_64");
        runtime_for(&at("jvm/temurin-17"), "17.0.16", "aarch64");
        runtime_for(&at("jvm/temurin-17-intel"), "17.0.16", "x86_64");
        runtime_for(&at("jvm/temurin-21"), "21.0.8", "aarch64");
        let runtimes = runtimes_on(&root, String::new(), MAC_ARM);
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
        let runtimes = &tree.runtimes;
        let find = |required| async move { auto(runtimes, &Needs::new(required)).await };
        let at = |rel, major, source| Some(tree.at(rel, major, source));

        let managed_21 = at("managed/21/Contents/Home/bin/java", 21, Source::Managed);
        assert_eq!(find(21).await, managed_21);
        let managed_17 = at("managed/17/bin/java", 17, Source::Managed);
        assert_eq!(find(17).await, managed_17);
        // Java 16 has no runtime here; 17 fits, 21 is further away.
        assert_eq!(find(16).await, managed_17);
        assert_eq!(find(11).await, at("bin/java", 11, Source::Path));
        let java_8 = at("jvm/temurin-8/bin/java", 8, Source::JavaHome);
        assert_eq!(find(8).await, java_8);
        // Nothing newer for 25, and never an older Java.
        assert_eq!(find(25).await, None);
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
        assert_eq!(auto(&runtimes, &Needs::new(8)).await, None);
    }

    #[tokio::test]
    async fn a_runtime_that_does_not_run_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // Looks complete, as a half-unpacked download by the Go app could.
        runtime(&tmp.path().join("managed/21"), None, Some("21.0.9"));
        let good = runtime(&tmp.path().join("jvm/temurin-21"), Some("21.0.1"), None);
        let runtimes = runtimes(tmp.path(), String::new());

        let found = auto(&runtimes, &Needs::new(21)).await.unwrap();
        assert_eq!((found.java, found.source), (good, Source::System));
    }

    /// `to` as a path relative to the current folder: up to the root, then
    /// down again.
    fn relative_to_cwd(to: &Path) -> PathBuf {
        let cwd = std::env::current_dir().unwrap();
        let ups = cwd
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count();
        let mut relative: PathBuf = std::iter::repeat_n("..", ups).collect();
        relative.push(to.strip_prefix("/").unwrap());
        relative
    }

    #[tokio::test]
    async fn empty_and_relative_path_entries_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let java = runtime(&tmp.path().join("jdk"), Some("21.0.1"), None);
        let relative = relative_to_cwd(java.parent().unwrap());
        assert!(relative.join("java").is_file(), "{}", relative.display());
        let runtimes = Runtimes {
            path: vec![PathBuf::new(), PathBuf::from("."), relative],
            ..runtimes(tmp.path(), String::new())
        };
        assert_eq!(runtimes.installed().await, []);
    }

    #[tokio::test]
    async fn an_explicit_java_wins_when_it_fits() {
        let tree = tree();
        let runtimes = &tree.runtimes;
        let find = |required, explicit: PathBuf| async move {
            let choice = Choice {
                settings: Some(&explicit),
                ..Choice::default()
            };
            let pick = runtimes.find(&Needs::new(required), choice).await?;
            assert_eq!(pick.stale, None);
            Ok::<_, Error>(pick.runtime)
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
                    origin: Origin::Settings,
                    found: 8,
                    required: 17,
                    ..
                })
            ),
            "{err:?}"
        );
        let err = find(17, tree.path("gone/bin/java")).await.unwrap_err();
        assert!(matches!(err, Error::ExplicitMissing { .. }), "{err:?}");
        assert!(
            err.to_string().starts_with("the Java set in Settings ("),
            "{err}"
        );
        let err = find(8, tree.path("jvm/broken/bin/java")).await;
        assert!(matches!(err, Err(Error::ExplicitBroken { .. })), "{err:?}");
        let err = find(8, tree.path("jvm/not-java")).await;
        assert!(matches!(err, Err(Error::ExplicitBroken { .. })), "{err:?}");
    }

    #[tokio::test]
    async fn launch_wrapper_versions_refuse_a_newer_java() {
        let tree = tree();
        let mut version = version("1.8.9.json");
        version.main_class = "net.minecraft.launchwrapper.Launch".into();
        // No natives, so any architecture runs it, on any computer.
        version.libraries.clear();
        let needs = tree.runtimes.needs(&version);
        assert_eq!(needs.newest, Some(8));

        let java_11 = tree.path("jvm/openjdk-11");
        let choice = Choice {
            settings: Some(&java_11),
            ..Choice::default()
        };
        let err = tree.runtimes.find(&needs, choice).await.unwrap_err();
        assert!(
            matches!(
                err,
                Error::ExplicitTooNew {
                    found: 11,
                    newest: 8,
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(
            err.to_string()
                .ends_with("is Java 11, but this version only runs on Java 8"),
            "{err}"
        );
        // By itself it picks Java 8, as for any Java 8 version.
        let java_8 = tree.at("jvm/temurin-8/bin/java", 8, Source::JavaHome);
        assert_eq!(auto(&tree.runtimes, &needs).await, Some(java_8));
    }

    #[tokio::test]
    async fn a_stale_instance_java_is_set_aside() {
        let mut tree = tree();
        tree.runtimes.go_managed = Some(tree.path("go-config/mctui/java"));
        let runtimes = &tree.runtimes;
        let find = |required, instance: PathBuf, settings: Option<PathBuf>| async move {
            let choice = Choice {
                instance: Some(&instance),
                settings: settings.as_deref(),
            };
            runtimes.find(&Needs::new(required), choice).await
        };
        let stale = |rel: &str, why| {
            Some(StaleJava {
                path: tree.path(rel),
                why,
            })
        };
        let managed_17 = Some(tree.at("managed/17/bin/java", 17, Source::Managed));

        // Gone since (a Homebrew upgrade): mctui picks as if it weren't set.
        let pick = find(17, tree.path("gone/bin/java"), None).await.unwrap();
        let want = Pick {
            runtime: managed_17.clone(),
            stale: stale("gone/bin/java", Staleness::Missing),
        };
        assert_eq!(pick, want);

        // In a managed folder, mctui's or the Go app's: it was an automatic
        // pick, even where it still works.
        let pinned = "managed/17/bin/java";
        let pick = find(17, tree.path(pinned), None).await.unwrap();
        let want = Pick {
            runtime: managed_17.clone(),
            stale: stale(pinned, Staleness::Managed),
        };
        assert_eq!(pick, want);
        let go_pinned = "go-config/mctui/java/17/bin/java";
        let pick = find(17, tree.path(go_pinned), None).await.unwrap();
        assert_eq!(pick.stale, stale(go_pinned, Staleness::Managed));

        // Too old for the version: the Java in Settings takes over.
        let old = "jvm/temurin-8/bin/java";
        let settings = tree.path("jvm/openjdk-11");
        let pick = find(11, tree.path(old), Some(settings)).await.unwrap();
        let too_old = Staleness::TooOld {
            found: 8,
            required: 11,
        };
        let want = Pick {
            runtime: Some(tree.at("jvm/openjdk-11/bin/java", 11, Source::Explicit)),
            stale: stale(old, too_old),
        };
        assert_eq!(pick, want);

        // One that fits is the user's choice.
        let pick = find(8, tree.path("jvm/openjdk-11"), None).await.unwrap();
        let want = tree.at("jvm/openjdk-11/bin/java", 11, Source::Explicit);
        assert_eq!((pick.runtime, pick.stale), (Some(want), None));

        // One that's there but broken is an error, and says whose it is.
        let err = find(8, tree.path("jvm/broken/bin/java"), None)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                Error::ExplicitBroken {
                    origin: Origin::Instance,
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(
            err.to_string().starts_with("this instance's Java ("),
            "{err}"
        );
    }

    #[tokio::test]
    async fn apple_silicon_runs_old_versions_on_an_intel_java() {
        let tree = mac_tree();
        let runtimes = &tree.runtimes;
        let pick = |version: VersionJson| async move {
            let needs = runtimes.needs(&version);
            auto(runtimes, &needs).await.map(|r| (r.java, r.arch))
        };
        let at = |rel: &str, arch| Some((tree.path(rel), arch));

        // LWJGL 2 and 3.2.1 have no arm64 natives for macOS.
        let intel_8 = at("jvm/temurin-8/bin/java", Arch::X86_64);
        assert_eq!(pick(version("1.16.5.json")).await, intel_8);
        assert_eq!(pick(version("1.8.9.json")).await, intel_8);
        let intel_17 = at("jvm/temurin-17-intel/bin/java", Arch::X86_64);
        assert_eq!(pick(old_natives_on_java(17)).await, intel_17);
        // No Intel Java 16 or newer but 17: that one, not the arm64 21.
        assert_eq!(pick(old_natives_on_java(16)).await, intel_17);

        // From 1.19 the natives run natively, and arm64 comes first.
        let arm_17 = at("jvm/temurin-17/bin/java", Arch::Arm64);
        assert_eq!(pick(new_natives_on_java(17)).await, arm_17);
        let arm_21 = at("jvm/temurin-21/bin/java", Arch::Arm64);
        assert_eq!(pick(version("1.21.8.json")).await, arm_21);

        // Nothing Intel fits Java 21: install one.
        assert_eq!(pick(old_natives_on_java(21)).await, None);
    }

    #[tokio::test]
    async fn an_intel_mac_is_unchanged() {
        let tree = mac_tree();
        let runtimes = Runtimes {
            os: Os::MacOs,
            arch: Arch::X86_64,
            ..tree.runtimes.clone()
        };
        let needs = runtimes.needs(&version("1.16.5.json"));
        assert_eq!(needs, Needs::new(8));
        let java = auto(&runtimes, &needs).await.unwrap();
        assert_eq!(java.java, tree.path("jvm/temurin-8/bin/java"));
    }

    #[tokio::test]
    async fn an_explicit_java_of_the_wrong_architecture_is_a_clear_error() {
        let tree = mac_tree();
        let runtimes = &tree.runtimes;
        let find = |version: VersionJson, instance: Option<PathBuf>, settings: Option<PathBuf>| async move {
            let choice = Choice {
                instance: instance.as_deref(),
                settings: settings.as_deref(),
            };
            runtimes.find(&runtimes.needs(&version), choice).await
        };

        let arm_8 = tree.path("jvm/zulu-8/bin/java");
        let err = find(version("1.16.5.json"), None, Some(arm_8))
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                Error::ExplicitWrongArch {
                    origin: Origin::Settings,
                    found: Arch::Arm64,
                    required: Arch::X86_64,
                    ..
                }
            ),
            "{err:?}"
        );
        let arm_17 = tree.path("jvm/temurin-17");
        let err = find(old_natives_on_java(17), Some(arm_17), None)
            .await
            .unwrap_err();
        let message = err.to_string();
        assert!(
            message.starts_with("this instance's Java (") && message.contains("is an arm64 Java"),
            "{message}"
        );

        // The right one is fine, and so is an arm64 one for new versions.
        let intel_17 = tree.path("jvm/temurin-17-intel/bin/java");
        let pick = find(old_natives_on_java(17), Some(intel_17), None)
            .await
            .unwrap();
        assert_eq!(pick.runtime.map(|r| r.arch), Some(Arch::X86_64));
        let arm_21 = tree.path("jvm/temurin-21/bin/java");
        let pick = find(version("1.21.8.json"), None, Some(arm_21))
            .await
            .unwrap();
        assert_eq!(pick.runtime.map(|r| r.arch), Some(Arch::Arm64));
    }
}

mod install {
    use sha2::Digest as _;

    use super::*;

    /// A runtime archive as Adoptium ships it: one top-level folder holding
    /// `bin/java` and a `release` file.
    fn jre_tar_gz(major: u32, extra: &[(&str, &[u8])]) -> Vec<u8> {
        jre_tar_gz_for(major, None, extra)
    }

    /// [`jre_tar_gz`], with a `release` file that says it's built for
    /// `os_arch`, if given.
    fn jre_tar_gz_for(major: u32, os_arch: Option<&str>, extra: &[(&str, &[u8])]) -> Vec<u8> {
        let java = format!("#!/bin/sh\necho 'openjdk version \"{major}.0.4\"' >&2\n");
        let os_arch = os_arch.map_or_else(String::new, |a| format!("OS_ARCH=\"{a}\"\n"));
        let release = format!("JAVA_VERSION=\"{major}.0.4\"\n{os_arch}");
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

    /// Where the mock Adoptium serves Java `major` for Adoptium's `arch`.
    fn route(major: u32, arch: Option<&str>) -> String {
        format!("/download/{major}-{}.tar.gz", arch.unwrap_or("any"))
    }

    /// Java `major` as listed on the mock Adoptium.
    struct Listing<'a> {
        major: u32,
        /// Adoptium's OS and CPU names; any if `None`.
        platform: Option<(&'a str, &'a str)>,
        /// The archive's file name, as Adoptium gives it.
        name: &'a str,
    }

    impl Listing<'_> {
        fn any(major: u32, name: &str) -> Listing<'_> {
            Listing {
                major,
                platform: None,
                name,
            }
        }
    }

    /// Lists an archive of `size` bytes published with `checksum` on the
    /// mock Adoptium, in the shape of a real response.
    async fn publish(server: &MockServer, listing: &Listing<'_>, checksum: &str, size: usize) {
        let Listing {
            major,
            platform,
            name,
        } = *listing;
        let fixture = include_str!("fixtures/java/adoptium-21-linux-x64.json");
        let mut body: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let package = &mut body[0]["binaries"][0]["package"];
        package["name"] = name.into();
        let route = route(major, platform.map(|(_, arch)| arch));
        package["link"] = format!("{}{route}", server.uri()).into();
        package["checksum"] = checksum.into();
        package["size"] = size.into();
        let mut mock = Mock::given(method("GET"))
            .and(path(format!("/v3/assets/feature_releases/{major}/ga")))
            .and(query_param("image_type", "jre"))
            .and(query_param("jvm_impl", "hotspot"))
            .and(query_param("vendor", "eclipse"));
        if let Some((os, arch)) = platform {
            mock = mock
                .and(query_param("os", os))
                .and(query_param("architecture", arch));
        }
        mock.respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    /// Serves a download at `route`.
    async fn serve(server: &MockServer, route: &str, download: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(download)
            .mount(server)
            .await;
    }

    /// Lists `archive` as described and serves it, published with its own
    /// checksum unless `checksum` says otherwise.
    async fn offer(
        server: &MockServer,
        listing: &Listing<'_>,
        archive: Vec<u8>,
        checksum: Option<&str>,
    ) {
        let checksum = checksum.map_or_else(|| sha256(&archive), str::to_owned);
        publish(server, listing, &checksum, archive.len()).await;
        let route = route(listing.major, listing.platform.map(|(_, arch)| arch));
        let download = ResponseTemplate::new(200).set_body_bytes(archive);
        serve(server, &route, download).await;
    }

    /// A mock Adoptium with `archive` as Java `major` for any platform. Like
    /// the real one, it answers 404 for anything else.
    async fn adoptium(major: u32, archive: Vec<u8>, checksum: Option<&str>) -> MockServer {
        let server = MockServer::start().await;
        let name = format!("OpenJDK{major}U-jre_hotspot.tar.gz");
        offer(&server, &Listing::any(major, &name), archive, checksum).await;
        server
    }

    async fn adoptium_with(major: u32, archive: Vec<u8>) -> MockServer {
        adoptium(major, archive, None).await
    }

    fn install<'a>(
        runtimes: &'a Runtimes,
        http: &'a Http,
        needs: &'a Needs,
    ) -> impl Future<Output = Result<Runtime, Error>> + 'a {
        runtimes.install(http, needs, |_| {})
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
            .install(&Http::new().unwrap(), &Needs::new(21), |p| events.push(p))
            .await
            .unwrap();

        let managed = tmp.path().join("managed");
        let want = Runtime {
            java: managed.join("21/bin").join(JAVA_EXE),
            major: 21,
            arch: Arch::current(),
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
        assert_eq!(auto(&runtimes, &Needs::new(21)).await, Some(want));
    }

    #[tokio::test]
    async fn a_bad_checksum_installs_nothing() {
        let wrong = sha256(b"something else");
        let server = adoptium(21, jre_tar_gz(21, &[]), Some(&wrong)).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let err = install(&runtimes, &Http::new().unwrap(), &Needs::new(21)).await;
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

        let err = install(&runtimes, &Http::new().unwrap(), &Needs::new(21)).await;
        assert!(matches!(err, Err(Error::UnsafeArchive { .. })), "{err:?}");
        assert!(names_in(&tmp.path().join("managed")).is_empty());
        assert!(!tmp.path().join("evil").exists());
    }

    /// Adoptium's file name comes off the network: were it used as a path,
    /// the download would overwrite whatever it names, and the cleanup after
    /// unpacking delete it.
    #[tokio::test]
    async fn the_download_is_named_here_not_by_adoptium() {
        let tmp = tempfile::tempdir().unwrap();
        let beside = tmp.path().join("victim.tar.gz");
        let absolute = tmp.path().join("elsewhere.tar.gz");
        let absolute_name = absolute.to_string_lossy().into_owned();
        let http = Http::new().unwrap();
        for (name, victim) in [("../victim.tar.gz", &beside), (&*absolute_name, &absolute)] {
            fs::write(victim, "the user's file").unwrap();
            let server = MockServer::start().await;
            offer(&server, &Listing::any(21, name), jre_tar_gz(21, &[]), None).await;
            let runtimes = runtimes(tmp.path(), server.uri());

            install(&runtimes, &http, &Needs::new(21)).await.unwrap();
            let kept = fs::read_to_string(victim).unwrap();
            assert_eq!(kept, "the user's file", "{name}");
            assert_eq!(names_in(&tmp.path().join("managed")), ["21"], "{name}");
        }
        assert_eq!(
            names_in(tmp.path()),
            ["elsewhere.tar.gz", "managed", "victim.tar.gz"]
        );
    }

    #[tokio::test]
    async fn only_tar_gz_and_zip_downloads_are_taken() {
        let server = MockServer::start().await;
        let msi = "OpenJDK21U-jre_x64_windows_hotspot.msi";
        offer(&server, &Listing::any(21, msi), jre_tar_gz(21, &[]), None).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let err = install(&runtimes, &Http::new().unwrap(), &Needs::new(21)).await;
        assert!(
            matches!(&err, Err(Error::UnknownArchive { name }) if name == msi),
            "{err:?}"
        );
        assert!(!tmp.path().join("managed").exists());
    }

    #[tokio::test]
    async fn the_wrong_java_in_the_archive_is_refused() {
        // Adoptium listing a Java 17 archive as Java 21.
        let server = adoptium_with(21, jre_tar_gz(17, &[])).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let err = install(&runtimes, &Http::new().unwrap(), &Needs::new(21)).await;
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

        let runtime = install(&runtimes, &Http::new().unwrap(), &Needs::new(16))
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

        let err = install(&runtimes, &Http::new().unwrap(), &Needs::new(8)).await;
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

        install(&runtimes, &Http::new().unwrap(), &Needs::new(21))
            .await
            .unwrap();
        assert_eq!(names_in(&old), ["bin", "lib", "release"]);
        assert_eq!(names_in(&tmp.path().join("managed")), ["21"]);
    }

    #[tokio::test]
    async fn cancelling_leaves_nothing_behind() {
        let server = MockServer::start().await;
        let archive = jre_tar_gz(21, &[]);
        let listing = Listing::any(21, "OpenJDK21U-jre_hotspot.tar.gz");
        publish(&server, &listing, &sha256(&archive), archive.len()).await;
        let stalled = ResponseTemplate::new(200)
            .set_body_bytes(archive)
            .set_delay(Duration::from_secs(30));
        serve(&server, &route(21, None), stalled).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes(tmp.path(), server.uri());

        let (http, needs) = (Http::new().unwrap(), Needs::new(21));
        let pending = install(&runtimes, &http, &needs);
        let cancelled = tokio::time::timeout(Duration::from_millis(500), pending).await;
        assert!(cancelled.is_err(), "the download should still be waiting");
        assert!(names_in(&tmp.path().join("managed")).is_empty());
    }

    /// A mock Adoptium with Java 8 for Intel Macs only, as the real one has,
    /// and Java 17 for both.
    async fn mac_adoptium() -> MockServer {
        let server = MockServer::start().await;
        for (major, arch, os_arch) in [
            (8, "x64", "x86_64"),
            (17, "x64", "x86_64"),
            (17, "aarch64", "aarch64"),
        ] {
            let name = format!("OpenJDK{major}U-jre_{arch}_mac_hotspot.tar.gz");
            let listing = Listing {
                major,
                platform: Some(("mac", arch)),
                name: &name,
            };
            let archive = jre_tar_gz_for(major, Some(os_arch), &[]);
            offer(&server, &listing, archive, None).await;
        }
        server
    }

    #[tokio::test]
    async fn apple_silicon_installs_an_intel_java_for_old_versions() {
        let server = mac_adoptium().await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes_on(tmp.path(), server.uri(), MAC_ARM);
        let http = Http::new().unwrap();
        let managed = tmp.path().join("managed");

        // 1.16.5, and 1.17 to 1.18.2: Intel, each beside a native one.
        let needs = runtimes.needs(&version("1.16.5.json"));
        let java_8 = install(&runtimes, &http, &needs).await.unwrap();
        assert_eq!((java_8.major, java_8.arch), (8, Arch::X86_64));
        assert!(java_8.java.starts_with(managed.join("8-x86_64")));
        let needs_17 = runtimes.needs(&old_natives_on_java(17));
        let intel_17 = install(&runtimes, &http, &needs_17).await.unwrap();
        assert_eq!((intel_17.major, intel_17.arch), (17, Arch::X86_64));
        assert!(intel_17.java.starts_with(managed.join("17-x86_64")));

        // 1.19 and on: arm64.
        let needs = runtimes.needs(&new_natives_on_java(17));
        let arm_17 = install(&runtimes, &http, &needs).await.unwrap();
        assert_eq!((arm_17.major, arm_17.arch), (17, Arch::Arm64));
        assert!(arm_17.java.starts_with(managed.join("17")));
        assert_eq!(names_in(&managed), ["17", "17-x86_64", "8-x86_64"]);

        // What was installed is what's found next time.
        #[cfg(unix)]
        {
            assert_eq!(auto(&runtimes, &needs_17).await, Some(intel_17));
            assert_eq!(auto(&runtimes, &needs).await, Some(arm_17));
        }
    }

    #[tokio::test]
    async fn a_download_built_for_another_architecture_is_refused() {
        // Listed as Intel, but arm64 inside.
        let server = MockServer::start().await;
        let listing = Listing {
            major: 17,
            platform: Some(("mac", "x64")),
            name: "OpenJDK17U-jre_x64_mac_hotspot.tar.gz",
        };
        offer(
            &server,
            &listing,
            jre_tar_gz_for(17, Some("aarch64"), &[]),
            None,
        )
        .await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = runtimes_on(tmp.path(), server.uri(), MAC_ARM);

        let needs = runtimes.needs(&old_natives_on_java(17));
        let err = install(&runtimes, &Http::new().unwrap(), &needs).await;
        assert!(
            matches!(err, Err(Error::BadDownload { major: 17 })),
            "{err:?}"
        );
        assert!(names_in(&tmp.path().join("managed")).is_empty());
    }
}
