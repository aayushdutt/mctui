//! Mojang's version metadata: decoding real (trimmed) responses from
//! `tests/fixtures/mojang`, rules over them, and the client against a mock
//! server, online and off.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use mctui_core::mojang::rules::{Arch, Env, Feature, Os};
use mctui_core::mojang::{
    Argument, Error, Extract, Library, Logging, Manifest, Mojang, VersionEntry, VersionJson,
    VersionKind,
};
use mctui_core::net::{self, Http};
use mctui_core::paths::DataDir;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mojang")
        .join(name);
    std::fs::read(path).unwrap()
}

fn version(name: &str) -> VersionJson {
    serde_json::from_slice(&fixture(name)).unwrap()
}

fn env(os: Os, arch: Arch, os_version: &str) -> Env {
    Env {
        os,
        arch,
        os_version: os_version.to_owned(),
        features: Vec::new(),
    }
}

fn args_for<'a>(args: &'a [Argument], env: &Env) -> Vec<&'a str> {
    args.iter()
        .flat_map(|a| a.values_for(env))
        .map(String::as_str)
        .collect()
}

fn libraries_for<'a>(version: &'a VersionJson, env: &Env) -> Vec<&'a str> {
    version
        .libraries
        .iter()
        .filter(|l| l.applies(env))
        .map(|l| l.name.as_str())
        .collect()
}

fn natives_for<'a>(version: &'a VersionJson, env: &Env) -> Vec<&'a str> {
    version
        .libraries
        .iter()
        .filter(|l| l.applies(env))
        .filter_map(|l| l.native_artifact(env))
        .map(|a| a.path.rsplit('/').next().unwrap())
        .collect()
}

#[test]
fn manifest_decodes() {
    let manifest: Manifest = serde_json::from_slice(&fixture("version_manifest_v2.json")).unwrap();
    assert_eq!(manifest.latest.release, "26.3");
    assert_eq!(manifest.latest.snapshot, "26.4-snapshot-1");

    let kinds: Vec<(&str, &VersionKind)> = manifest
        .versions
        .iter()
        .map(|v| (v.id.as_str(), &v.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("26.4-snapshot-1", &VersionKind::Snapshot),
            ("26.3", &VersionKind::Release),
            ("1.21.8", &VersionKind::Release),
            ("1.16.5", &VersionKind::Release),
            ("1.14 Pre-Release 5", &VersionKind::Snapshot),
            ("1.8.9", &VersionKind::Release),
            ("b1.7.3", &VersionKind::OldBeta),
            ("rd-132211", &VersionKind::OldAlpha),
        ]
    );
    let entry = manifest.find("1.16.5").unwrap();
    assert_eq!(entry.sha1, "fba9f7833e858a1257d810d21a3a9e3c967f9077");
    assert_eq!(entry.release_time.to_string(), "2021-01-14T16:05:32Z");
    assert!(manifest.find("1.16.6").is_none());
}

#[test]
fn modern_release_decodes() {
    let v = version("1.21.8.json");
    assert_eq!((v.id.as_str(), &v.kind), ("1.21.8", &VersionKind::Release));
    assert_eq!(v.main_class, "net.minecraft.client.main.Main");
    assert_eq!(v.inherits_from, None);
    assert_eq!(v.minecraft_arguments, None);
    assert_eq!(v.assets.as_deref(), Some("26"));
    let index = v.asset_index.as_ref().unwrap();
    assert_eq!((index.id.as_str(), index.total_size), ("26", 449_007_950));
    let java = v.java_version.as_ref().unwrap();
    assert_eq!(
        (java.component.as_str(), java.major_version),
        ("java-runtime-delta", 21)
    );
    let downloads = v.downloads.as_ref().unwrap();
    assert_eq!(
        downloads.client.sha1,
        "a19d9badbea944a4369fd0059e53bf7286597576"
    );
    assert!(downloads.client_mappings.is_some() && downloads.server.is_some());
    let logging = v.logging.client.as_ref().unwrap();
    assert_eq!(logging.argument, "-Dlog4j.configurationFile=${path}");
    assert_eq!(
        (logging.file.id.as_str(), logging.kind.as_str()),
        ("client-1.21.2.xml", "log4j2-xml")
    );
    assert_eq!(v.compliance_level, 1);
    assert_eq!(v.release_time.unwrap().to_string(), "2025-07-17T12:04:02Z");
    assert!(v.arguments.default_user_jvm.is_empty());

    // Every library knows its jar.
    for lib in &v.libraries {
        let artifact = lib.artifact().unwrap().unwrap();
        assert!(artifact.url.starts_with("https://libraries.minecraft.net/"));
        assert!(artifact.sha1.is_some() && artifact.size.is_some());
    }
}

#[test]
fn modern_rules_pick_the_right_natives_and_arguments() {
    let v = version("1.21.8.json");
    let mac = env(Os::MacOs, Arch::Arm64, "");
    let linux = env(Os::Linux, Arch::X86_64, "");
    let win32 = env(Os::Windows, Arch::X86, "10.0");

    let natives = |env: &Env| -> Vec<String> {
        libraries_for(&v, env)
            .into_iter()
            .filter_map(|name| name.split_once(":natives-").map(|(_, c)| c.to_owned()))
            .collect()
    };
    assert_eq!(natives(&mac), ["macos", "macos-arm64"]);
    assert_eq!(natives(&linux), ["linux"]);
    assert_eq!(natives(&win32), ["windows", "windows-arm64", "windows-x86"]);
    assert!(libraries_for(&v, &mac).contains(&"ca.weblite:java-objc-bridge:1.1"));
    assert!(!libraries_for(&v, &linux).contains(&"ca.weblite:java-objc-bridge:1.1"));

    let jvm = |env: &Env| args_for(&v.arguments.jvm, env);
    assert!(jvm(&mac).contains(&"-XstartOnFirstThread"));
    assert!(!jvm(&linux).contains(&"-XstartOnFirstThread"));
    assert!(jvm(&win32).contains(&"-Xss1M"));
    assert!(
        jvm(&win32)
            .iter()
            .any(|a| a.starts_with("-XX:HeapDumpPath="))
    );
    assert!(!jvm(&linux).contains(&"-Xss1M"));
    assert_eq!(jvm(&linux).last(), Some(&"${classpath}"));

    let game = |env: &Env| args_for(&v.arguments.game, env);
    assert!(!game(&linux).contains(&"--demo"));
    assert!(!game(&linux).contains(&"--width"));
    assert!(!game(&linux).contains(&"--quickPlayMultiplayer"));
    let custom = Env {
        features: vec![Feature::CustomResolution, Feature::QuickPlayMultiplayer],
        ..linux.clone()
    };
    let with_features = game(&custom);
    assert!(with_features.contains(&"--width") && with_features.contains(&"${resolution_height}"));
    assert!(with_features.contains(&"--quickPlayMultiplayer"));
    assert!(!with_features.contains(&"--quickPlaySingleplayer"));
}

#[test]
fn snapshot_with_suggested_jvm_options_decodes() {
    let v = version("26.4-snapshot-1.json");
    assert_eq!(v.kind, VersionKind::Snapshot);
    assert_eq!(v.java_version.as_ref().unwrap().major_version, 25);

    let suggested = |env: &Env| args_for(&v.arguments.default_user_jvm, env);
    let zgc = &"-XX:+UseZGC";
    let g1 = &"-XX:+UseG1GC";
    let mac = suggested(&env(Os::MacOs, Arch::Arm64, ""));
    assert!(mac.contains(&"-Xmx4G") && mac.contains(zgc) && !mac.contains(g1));
    let win11 = suggested(&env(Os::Windows, Arch::X86_64, "10.0.22631"));
    assert!(win11.contains(zgc) && !win11.contains(g1));
    let old_win = suggested(&env(Os::Windows, Arch::X86_64, "10.0.10586"));
    assert!(old_win.contains(g1) && !old_win.contains(zgc));
}

#[test]
fn legacy_release_decodes() {
    let v = version("1.8.9.json");
    let legacy = v.minecraft_arguments.as_deref().unwrap();
    assert!(legacy.starts_with("--username ${auth_player_name} --version ${version_name}"));
    assert!(v.arguments.game.is_empty() && v.arguments.jvm.is_empty());
    assert_eq!(v.assets.as_deref(), Some("1.8"));
    assert_eq!(v.compliance_level, 0);

    // LWJGL 2 natives come from classifiers, Twitch's by bitness.
    let natives = |os, arch| natives_for(&v, &env(os, arch, ""));
    assert_eq!(
        natives(Os::Windows, Arch::X86),
        [
            "lwjgl-platform-2.9.4-nightly-20150209-natives-windows.jar",
            "jinput-platform-2.0.5-natives-windows.jar",
            "twitch-platform-6.5-natives-windows-32.jar",
            "twitch-external-platform-4.5-natives-windows-32.jar",
        ]
    );
    assert_eq!(
        natives(Os::Windows, Arch::X86_64)[2],
        "twitch-platform-6.5-natives-windows-64.jar"
    );
    assert_eq!(
        natives(Os::MacOs, Arch::X86_64),
        [
            "lwjgl-platform-2.9.2-nightly-20140822-natives-osx.jar",
            "jinput-platform-2.0.5-natives-osx.jar",
            "twitch-platform-6.5-natives-osx.jar",
        ]
    );
    let platform = v
        .libraries
        .iter()
        .find(|l| l.name.ends_with("lwjgl-platform:2.9.2-nightly-20140822"))
        .unwrap();
    assert_eq!(platform.artifact(), Ok(None), "natives only");
    assert_eq!(platform.extract.exclude, ["META-INF/"]);
}

#[test]
fn natives_by_classifier_follow_the_rules() {
    let v = version("1.16.5.json");
    let mac = env(Os::MacOs, Arch::X86_64, "");
    let linux = env(Os::Linux, Arch::X86_64, "");
    assert_eq!(
        natives_for(&v, &mac),
        [
            "lwjgl-3.2.1-natives-macos.jar",
            "lwjgl-glfw-3.2.1-natives-macos.jar",
            "java-objc-bridge-1.0.0-natives-osx.jar",
        ]
    );
    assert_eq!(
        natives_for(&v, &linux),
        [
            "lwjgl-3.2.2-natives-linux.jar",
            "lwjgl-glfw-3.2.2-natives-linux.jar",
            "text2speech-1.11.3-natives-linux.jar",
        ]
    );
}

/// The Go app cached version JSONs re-encoded through its own structs, which
/// dropped `logging`, `extract`, and `complianceLevel`. Those files still
/// read, so an offline launch after upgrading works.
#[test]
fn go_written_cache_decodes() {
    let from_go = version("1.16.5.go-cache.json");
    let mut expected = version("1.16.5.json");
    expected.logging = Logging::default();
    expected.compliance_level = 0;
    for lib in &mut expected.libraries {
        lib.extract = Extract::default();
    }
    assert_eq!(from_go, expected);
}

/// A mock of Mojang's servers: the fixture manifest, rewritten so version
/// URLs point at the mock and checksums match the fixture files.
struct Server {
    mock: MockServer,
    manifest_url: String,
}

impl Server {
    async fn start() -> Self {
        let mock = MockServer::start().await;
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fixture("version_manifest_v2.json")).unwrap();
        for entry in manifest["versions"].as_array_mut().unwrap() {
            let id = entry["id"].as_str().unwrap().to_owned();
            let Ok(body) = std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("tests/fixtures/mojang/{id}.json")),
            ) else {
                continue;
            };
            entry["url"] = json!(format!("{}/v/{id}.json", mock.uri()));
            entry["sha1"] = json!(sha1_hex(&body));
            Mock::given(method("GET"))
                .and(path(format!("/v/{id}.json")))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&mock)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/manifest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(manifest))
            .mount(&mock)
            .await;
        let manifest_url = format!("{}/manifest.json", mock.uri());
        Self { mock, manifest_url }
    }

    async fn requests_to(&self, route: &str) -> usize {
        let requests = self.mock.received_requests().await.unwrap_or_default();
        requests.iter().filter(|r| r.url.path() == route).count()
    }
}

fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::Digest as _;
    use std::fmt::Write as _;
    sha1::Sha1::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn client(data: &DataDir, manifest_url: &str) -> Mojang {
    Mojang::with_manifest_url(Http::new().unwrap(), data, manifest_url)
}

/// Stands in for an unreachable Mojang by failing every request. (Failing
/// fast: connection errors are retried with backoff, which only slows tests.)
async fn dead_server() -> MockServer {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;
    mock
}

#[tokio::test]
async fn resolve_verifies_and_caches_the_raw_json() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let mojang = client(&data, &server.manifest_url);

    let got = mojang.resolve("1.21.8").await.unwrap();
    assert!(!got.stale);
    assert_eq!(got.value, version("1.21.8.json"));
    // Cached byte for byte, not re-encoded.
    let cached = std::fs::read(data.cache_dir().join("versions/1.21.8.json")).unwrap();
    assert_eq!(cached, fixture("1.21.8.json"));
    assert!(data.cache_dir().join("version_manifest_v2.json").is_file());

    // The cached copy matches the manifest's checksum: no second download.
    let again = mojang.resolve("1.21.8").await.unwrap();
    assert_eq!(again, got);
    assert_eq!(server.requests_to("/v/1.21.8.json").await, 1);
    assert_eq!(server.requests_to("/manifest.json").await, 2);
}

#[tokio::test]
async fn resolve_uses_the_cache_when_offline() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let online = client(&data, &server.manifest_url)
        .resolve("1.16.5")
        .await
        .unwrap();

    let dead = dead_server().await;
    let offline = client(&data, &format!("{}/manifest.json", dead.uri()));
    let got = offline.resolve("1.16.5").await.unwrap();
    assert!(got.stale);
    assert_eq!(got.value, online.value);
    let manifest = offline.manifest().await.unwrap();
    assert!(manifest.stale);
    assert_eq!(manifest.value.latest.release, "26.3");
    assert_eq!(
        offline.cached_version("1.16.5").await.unwrap(),
        online.value
    );
}

#[tokio::test]
async fn a_go_cache_is_used_offline_and_replaced_online() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let cached = data.cache_dir().join("versions/1.16.5.json");
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    std::fs::write(&cached, fixture("1.16.5.go-cache.json")).unwrap();

    // Offline with no manifest ever fetched: the Go app's copy is all there is.
    let dead = dead_server().await;
    let got = client(&data, &format!("{}/manifest.json", dead.uri()))
        .resolve("1.16.5")
        .await
        .unwrap();
    assert!(got.stale);
    assert_eq!(got.value, version("1.16.5.go-cache.json"));

    // Online, its checksum doesn't match the manifest, so the real JSON
    // replaces it, and with it what Go dropped.
    let server = Server::start().await;
    let got = client(&data, &server.manifest_url)
        .resolve("1.16.5")
        .await
        .unwrap();
    assert!(!got.stale);
    assert!(got.value.logging.client.is_some());
    assert_eq!(std::fs::read(&cached).unwrap(), fixture("1.16.5.json"));
}

#[tokio::test]
async fn a_bad_checksum_is_refused() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v/1.21.8.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture("1.21.8.json")))
        .mount(&mock)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let entry = VersionEntry {
        id: "1.21.8".into(),
        kind: VersionKind::Release,
        url: format!("{}/v/1.21.8.json", mock.uri()),
        sha1: "0000000000000000000000000000000000000000".into(),
        release_time: "2025-07-17T12:04:02Z".parse().unwrap(),
    };
    let err = client(&data, "unused").version(&entry).await.unwrap_err();
    assert!(
        matches!(&err, Error::Version { id, source: net::Error::Checksum { .. } } if id == "1.21.8"),
        "{err}"
    );
    assert!(!data.cache_dir().join("versions/1.21.8.json").exists());
}

#[tokio::test]
async fn errors_say_what_is_missing() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());

    let online = client(&data, &server.manifest_url);
    let err = online.resolve("1.99").await.unwrap_err();
    assert!(
        matches!(&err, Error::UnknownVersion(id) if id == "1.99"),
        "{err}"
    );
    let err = online.cached_version("1.21.8").await.unwrap_err();
    assert!(matches!(err, Error::NotCached(_)), "{err}");
    let err = online.cached_version("../../config").await.unwrap_err();
    assert!(matches!(err, Error::UnknownVersion(_)), "{err}");

    // Offline, with nothing cached.
    let dead = dead_server().await;
    let empty = tempfile::tempdir().unwrap();
    let offline = client(
        &DataDir::new(empty.path()),
        &format!("{}/manifest.json", dead.uri()),
    );
    let err = offline.resolve("1.21.8").await.unwrap_err();
    assert!(matches!(err, Error::Manifest(_)), "{err}");
    assert!(
        err.to_string()
            .starts_with("couldn't get the list of Minecraft versions")
    );
}

#[tokio::test]
async fn a_version_json_mojang_cant_serve_falls_back_to_the_cache() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let mojang = client(&data, &server.manifest_url);
    let manifest = mojang.manifest().await.unwrap().value;
    let entry = manifest.find("1.8.9").unwrap();

    // Nothing cached yet, and the file is gone from the server.
    let gone = VersionEntry {
        url: format!("{}/v/missing.json", server.mock.uri()),
        ..entry.clone()
    };
    let err = mojang.version(&gone).await.unwrap_err();
    assert!(
        matches!(&err, Error::Version { source, .. } if source.status().is_some()),
        "{err}"
    );

    // Cached once, then Mojang republishes with a new checksum and fails:
    // the old copy is better than nothing.
    mojang.version(entry).await.unwrap();
    let republished = VersionEntry {
        sha1: "1111111111111111111111111111111111111111".into(),
        ..gone
    };
    let got = mojang.version(&republished).await.unwrap();
    assert!(got.stale);
    assert_eq!(got.value, version("1.8.9.json"));
}

/// The TUI cancels by dropping the future; that mustn't leave a partial file
/// in the cache for a later offline launch to trip over.
#[tokio::test]
async fn cancelling_leaves_nothing_behind() {
    let mock = MockServer::start().await;
    let body = fixture("1.21.8.json");
    let entry = VersionEntry {
        id: "1.21.8".into(),
        kind: VersionKind::Release,
        url: format!("{}/v/1.21.8.json", mock.uri()),
        sha1: sha1_hex(&body),
        release_time: "2025-07-17T12:04:02Z".parse().unwrap(),
    };
    let slow = ResponseTemplate::new(200)
        .set_body_bytes(body)
        .set_delay(std::time::Duration::from_secs(10));
    Mock::given(method("GET"))
        .respond_with(slow)
        .mount(&mock)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());

    let mojang = client(&data, "unused");
    let fetch = mojang.version(&entry);
    let cancelled = tokio::time::timeout(std::time::Duration::from_millis(200), fetch).await;
    assert!(cancelled.is_err(), "still downloading");
    let versions = data.cache_dir().join("versions");
    assert!(versions.is_dir(), "the download had started");
    assert_eq!(std::fs::read_dir(versions).unwrap().count(), 0);
    assert!(matches!(
        mojang.cached_version("1.21.8").await,
        Err(Error::NotCached(_))
    ));
}

#[tokio::test]
async fn a_damaged_cache_is_reported_offline() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let cached = data.cache_dir().join("versions/1.21.8.json");
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    std::fs::write(&cached, "{\"id\": ").unwrap();
    let err = client(&data, "unused")
        .cached_version("1.21.8")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Cache(_)), "{err}");
}

/// Live: Mojang's real manifest and versions. Run with `--ignored`.
#[tokio::test]
#[ignore = "hits piston-meta.mojang.com"]
async fn live_mojang() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let mojang = Mojang::new(Http::new().unwrap(), &data);

    let manifest = mojang.manifest().await.unwrap();
    assert!(!manifest.stale);
    let latest = manifest.value.latest.release.clone();
    let env = Env::current();
    for id in [latest.as_str(), "1.16.5", "1.8.9", "rd-132211"] {
        let v = mojang.resolve(id).await.unwrap();
        assert!(!v.stale, "{id}");
        assert_eq!(v.value.id, id);
        assert!(
            v.value.downloads.is_some() && v.value.asset_index.is_some(),
            "{id}"
        );
        let libraries: Vec<&Library> = v
            .value
            .libraries
            .iter()
            .filter(|l| l.applies(&env))
            .collect();
        assert!(!libraries.is_empty(), "{id}");
        for lib in libraries {
            lib.artifact().unwrap();
        }
    }
}
