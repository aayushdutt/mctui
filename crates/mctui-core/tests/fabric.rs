//! Fabric: decoding real (trimmed) meta.fabricmc.net responses from
//! `tests/fixtures/fabric`, merging real profiles into real game versions,
//! and the client against mock servers, online and off.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use mctui_core::fabric::{self, Error, Fabric, GameVersion, LoaderVersion};
use mctui_core::mojang::rules::{Arch, Env, Os};
use mctui_core::mojang::{Mojang, VersionJson};
use mctui_core::net::Http;
use mctui_core::paths::DataDir;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KNOT: &str = "net.fabricmc.loader.impl.launch.knot.KnotClient";

fn fixture(rel: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel);
    std::fs::read(path).unwrap()
}

fn decode(rel: &str) -> VersionJson {
    serde_json::from_slice(&fixture(rel)).unwrap()
}

fn env(os: Os, arch: Arch) -> Env {
    Env {
        os,
        arch,
        os_version: String::new(),
        features: Vec::new(),
    }
}

#[test]
fn profile_decodes_as_a_version_json() {
    let profile = decode("fabric/profile_1.21.8_0.19.5.json");
    assert_eq!(profile.id, "fabric-loader-0.19.5-1.21.8");
    assert_eq!(profile.inherits_from.as_deref(), Some("1.21.8"));
    assert_eq!(profile.main_class, KNOT);
    // Fabric writes offsets without a colon: `2026-09-27T11:26:12+0000`.
    assert_eq!(
        profile.release_time.unwrap().to_string(),
        "2026-09-27T11:26:12Z"
    );
    assert!(profile.downloads.is_none() && profile.java_version.is_none());

    let asm = &profile.libraries[0];
    assert_eq!(asm.name, "org.ow2.asm:asm:9.10.1");
    let artifact = asm.artifact().unwrap().unwrap();
    assert_eq!(
        artifact.url,
        "https://maven.fabricmc.net/org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"
    );
    assert_eq!(
        artifact.sha1.as_deref(),
        Some("ada2141c0cc52ee8f5c48cd5fa4ce0e794f22236")
    );
    assert_eq!(artifact.size, Some(126_151));

    // Fabric publishes no checksum for its own jars.
    let loader = profile.libraries.last().unwrap();
    let artifact = loader.artifact().unwrap().unwrap();
    assert_eq!(
        artifact.path,
        "net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar"
    );
    assert_eq!((artifact.sha1, artifact.size), (None, None));
}

#[test]
fn merging_into_a_modern_release() {
    let parent = decode("mojang/1.21.8.json");
    let merged = fabric::merge(parent.clone(), decode("fabric/profile_1.21.8_0.19.5.json"));

    assert_eq!(merged.id, "1.21.8");
    assert_eq!(merged.main_class, KNOT);
    assert_eq!(merged.downloads, parent.downloads);
    assert_eq!(merged.logging, parent.logging);
    let jvm: Vec<&str> = merged
        .arguments
        .jvm
        .iter()
        .flat_map(|a| &a.values)
        .map(String::as_str)
        .collect();
    assert_eq!(
        jvm.last(),
        Some(&"-DFabricMcEmu= net.minecraft.client.main.Main ")
    );
    assert_eq!(merged.arguments.game, parent.arguments.game);

    // Fabric's ASM 9.10.1 replaces the game's 9.6; nothing else overlaps.
    let asm: Vec<&str> = merged
        .libraries
        .iter()
        .map(|l| l.name.as_str())
        .filter(|n| n.starts_with("org.ow2.asm:asm:"))
        .collect();
    assert_eq!(asm, ["org.ow2.asm:asm:9.10.1"]);
    assert_eq!(merged.libraries.len(), parent.libraries.len() - 1 + 8);
    for lib in &merged.libraries {
        let artifact = lib.artifact().unwrap().unwrap();
        assert!(artifact.url.starts_with("https://"), "{}", lib.name);
    }
}

/// Go bug L-6: merging deduplicated the game's own libraries, keeping LWJGL
/// 3.2.2 (not for macOS) and dropping 3.2.1 (macOS only), so Fabric on 1.14
/// to 1.18.2 couldn't start on a Mac.
#[test]
fn merging_keeps_every_library_of_the_game() {
    let parent = decode("mojang/1.16.5.json");
    let merged = fabric::merge(parent.clone(), decode("fabric/profile_1.16.5_0.19.5.json"));
    assert_eq!(
        merged.libraries[..parent.libraries.len()],
        parent.libraries[..]
    );

    let natives = |env: &Env| -> Vec<String> {
        merged
            .libraries
            .iter()
            .filter(|l| l.applies(env))
            .filter_map(|l| l.native_artifact(env))
            .map(|a| a.path.rsplit('/').next().unwrap().to_owned())
            .collect()
    };
    let mac = env(Os::MacOs, Arch::X86_64);
    assert_eq!(
        natives(&mac),
        [
            "lwjgl-3.2.1-natives-macos.jar",
            "lwjgl-glfw-3.2.1-natives-macos.jar",
            "java-objc-bridge-1.0.0-natives-osx.jar",
        ]
    );
    let on_mac: Vec<&str> = merged
        .libraries
        .iter()
        .filter(|l| l.applies(&mac))
        .map(|l| l.name.as_str())
        .collect();
    assert!(on_mac.contains(&"org.lwjgl:lwjgl:3.2.1"));
    assert!(!on_mac.contains(&"org.lwjgl:lwjgl:3.2.2"));
    assert!(on_mac.contains(&"net.fabricmc:fabric-loader:0.19.5"));

    let linux = env(Os::Linux, Arch::X86_64);
    assert_eq!(
        natives(&linux),
        [
            "lwjgl-3.2.2-natives-linux.jar",
            "lwjgl-glfw-3.2.2-natives-linux.jar",
            "text2speech-1.11.3-natives-linux.jar",
        ]
    );
}

fn loader_list() -> serde_json::Value {
    serde_json::from_slice(&fixture("fabric/loader_versions_1.21.8.json")).unwrap()
}

/// Mock Mojang and Fabric servers with the fixtures for 1.21.8.
struct Servers {
    mojang: MockServer,
    fabric: MockServer,
}

impl Servers {
    async fn start() -> Self {
        let mojang = MockServer::start().await;
        let body = fixture("mojang/1.21.8.json");
        let manifest = json!({
            "latest": {"release": "1.21.8", "snapshot": "1.21.8"},
            "versions": [{
                "id": "1.21.8", "type": "release", "url": format!("{}/v/1.21.8.json", mojang.uri()),
                "sha1": sha1_hex(&body), "releaseTime": "2025-07-17T12:04:02+00:00"
            }]
        });
        mount(
            &mojang,
            "/manifest.json",
            ResponseTemplate::new(200).set_body_json(manifest),
        )
        .await;
        mount(
            &mojang,
            "/v/1.21.8.json",
            ResponseTemplate::new(200).set_body_bytes(body),
        )
        .await;

        let fabric = MockServer::start().await;
        mount(
            &fabric,
            "/v2/versions/game",
            ResponseTemplate::new(200).set_body_bytes(fixture("fabric/game_versions.json")),
        )
        .await;
        mount(
            &fabric,
            "/v2/versions/loader/1.21.8",
            ResponseTemplate::new(200).set_body_json(loader_list()),
        )
        .await;
        mount(
            &fabric,
            "/v2/versions/loader/1.21.8/0.19.5/profile/json",
            ResponseTemplate::new(200).set_body_bytes(fixture("fabric/profile_1.21.8_0.19.5.json")),
        )
        .await;
        // What Fabric answers for versions it doesn't know.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(400).set_body_string("[]"))
            .with_priority(u8::MAX)
            .mount(&fabric)
            .await;
        Self { mojang, fabric }
    }

    fn clients(&self, data: &DataDir) -> (Mojang, Fabric) {
        let http = Http::new().unwrap();
        let manifest_url = format!("{}/manifest.json", self.mojang.uri());
        (
            Mojang::with_manifest_url(http.clone(), data, manifest_url),
            Fabric::with_meta_url(http, data, format!("{}/", self.fabric.uri())),
        )
    }

    async fn fabric_requests_to(&self, route: &str) -> usize {
        let requests = self.fabric.received_requests().await.unwrap_or_default();
        requests.iter().filter(|r| r.url.path() == route).count()
    }
}

async fn mount(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(response)
        .mount(server)
        .await;
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

#[tokio::test]
async fn lists_game_and_loader_versions() {
    let servers = Servers::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (_, fabric) = servers.clients(&DataDir::new(tmp.path()));

    let games = fabric.game_versions().await.unwrap();
    assert_eq!(games.len(), 6);
    assert_eq!(
        games[1],
        GameVersion {
            version: "26.3".into(),
            stable: true
        }
    );
    assert!(
        games
            .iter()
            .any(|g| g.version == "1.14 Pre-Release 5" && !g.stable)
    );

    let loaders = fabric.loader_versions("1.21.8").await.unwrap();
    let want =
        [("0.19.5", true), ("0.19.4", false), ("0.19.3", false)].map(|(v, stable)| LoaderVersion {
            version: v.into(),
            stable,
        });
    assert_eq!(loaders, want);
    assert_eq!(fabric::default_loader(&loaders), Some(&want[0]));

    assert_eq!(fabric.loader_versions("1.8.9").await.unwrap(), []);
}

#[tokio::test]
async fn version_ids_are_escaped_in_urls() {
    let fabric_mock = MockServer::start().await;
    let loaders = loader_list();
    mount(
        &fabric_mock,
        "/v2/versions/loader/1.14%20Pre-Release%205",
        ResponseTemplate::new(200).set_body_json(loaders),
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let fabric = Fabric::with_meta_url(
        Http::new().unwrap(),
        &DataDir::new(tmp.path()),
        fabric_mock.uri(),
    );
    let loaders = fabric.loader_versions("1.14 Pre-Release 5").await.unwrap();
    assert_eq!(loaders.len(), 3);
}

#[tokio::test]
async fn profiles_are_fetched_once_and_cached_raw() {
    let servers = Servers::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let (_, fabric) = servers.clients(&data);

    let profile = fabric.profile("1.21.8", "0.19.5").await.unwrap();
    assert_eq!(profile, decode("fabric/profile_1.21.8_0.19.5.json"));
    let cached = data
        .cache_dir()
        .join("versions/fabric-loader-0.19.5-1.21.8.json");
    assert_eq!(
        std::fs::read(&cached).unwrap(),
        fixture("fabric/profile_1.21.8_0.19.5.json")
    );

    assert_eq!(fabric.profile("1.21.8", "0.19.5").await.unwrap(), profile);
    let route = "/v2/versions/loader/1.21.8/0.19.5/profile/json";
    assert_eq!(servers.fabric_requests_to(route).await, 1);

    // Offline: still there.
    let dead = Fabric::with_meta_url(Http::new().unwrap(), &data, "http://127.0.0.1:9");
    assert_eq!(dead.profile("1.21.8", "0.19.5").await.unwrap(), profile);
}

#[tokio::test]
async fn profile_errors() {
    let servers = Servers::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let (_, fabric) = servers.clients(&data);

    let err = fabric.profile("1.21.8", "0.0.1").await.unwrap_err();
    assert!(matches!(err, Error::Unavailable { .. }), "{err}");
    assert_eq!(
        err.to_string(),
        "Fabric 0.0.1 isn't available for Minecraft 1.21.8"
    );

    // A profile for another game version is refused, and not cached.
    mount(
        &servers.fabric,
        "/v2/versions/loader/1.21.7/0.19.5/profile/json",
        ResponseTemplate::new(200).set_body_bytes(fixture("fabric/profile_1.21.8_0.19.5.json")),
    )
    .await;
    let err = fabric.profile("1.21.7", "0.19.5").await.unwrap_err();
    assert!(matches!(err, Error::WrongGame { .. }), "{err}");
    let versions = data.cache_dir().join("versions");
    assert!(!versions.join("fabric-loader-0.19.5-1.21.7.json").exists());

    let err = fabric.profile("1.21.8", "../../x").await.unwrap_err();
    assert!(matches!(err, Error::Unavailable { .. }), "{err}");
}

#[tokio::test]
async fn resolve_merges_and_works_offline() {
    let servers = Servers::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let (mojang, fabric) = servers.clients(&data);

    let online = fabric.resolve(&mojang, "1.21.8", "0.19.5").await.unwrap();
    assert!(!online.stale);
    assert_eq!(
        online.value,
        fabric::merge(
            decode("mojang/1.21.8.json"),
            decode("fabric/profile_1.21.8_0.19.5.json")
        )
    );

    // Both services gone: the caches carry the launch.
    let dead = MockServer::start().await;
    let http = Http::new().unwrap();
    let mojang = Mojang::with_manifest_url(http.clone(), &data, format!("{}/m", dead.uri()));
    let fabric = Fabric::with_meta_url(http, &data, dead.uri());
    let offline = fabric.resolve(&mojang, "1.21.8", "0.19.5").await.unwrap();
    assert!(offline.stale);
    assert_eq!(offline.value, online.value);

    // Offline, a loader that was never fetched can't be had.
    let err = fabric
        .resolve(&mojang, "1.21.8", "0.19.4")
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Unavailable { .. } | Error::Profile { .. }),
        "{err}"
    );
}

/// Live: meta.fabricmc.net and Mojang for real. Run with `--ignored`.
#[tokio::test]
#[ignore = "hits meta.fabricmc.net and piston-meta.mojang.com"]
async fn live_fabric() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path());
    let http = Http::new().unwrap();
    let mojang = Mojang::new(http.clone(), &data);
    let fabric = Fabric::new(http, &data);

    let games = fabric.game_versions().await.unwrap();
    let game = games.iter().find(|g| g.stable).unwrap().version.clone();
    let loaders = fabric.loader_versions(&game).await.unwrap();
    let loader = fabric::default_loader(&loaders).unwrap().version.clone();
    assert!(fabric.loader_versions("1.8.9").await.unwrap().is_empty());

    for game in [game.as_str(), "1.16.5", "1.14 Pre-Release 5"] {
        let merged = fabric.resolve(&mojang, game, &loader).await.unwrap();
        assert!(!merged.stale, "{game}");
        assert_eq!(merged.value.id, game);
        assert_eq!(merged.value.main_class, KNOT, "{game}");
        let env = Env::current();
        for lib in merged.value.libraries.iter().filter(|l| l.applies(&env)) {
            lib.artifact().unwrap();
        }
        assert!(
            merged
                .value
                .libraries
                .iter()
                .any(|l| l.name == format!("net.fabricmc:fabric-loader:{loader}"))
        );
    }
}
