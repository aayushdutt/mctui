//! Launching: plans worked out from real (trimmed) version JSONs per
//! platform, preparing a synthetic version against a mock server into a
//! temp data dir, and whole launches with a stand-in `java`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use mctui_core::account::Secret;
use mctui_core::config::Config;
use mctui_core::fabric;
use mctui_core::instance::{Instance, Loader};
use mctui_core::launch::{self, AssetLayout, Error, Identity, Plan, PlanInputs, Step};
use mctui_core::mojang::VersionJson;
use mctui_core::mojang::rules::{Arch, Env, Os};
use mctui_core::net::Hash;
use mctui_core::paths::DataDir;

const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

fn fixture(rel: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel);
    std::fs::read(path).unwrap()
}

fn version(rel: &str) -> VersionJson {
    serde_json::from_slice(&fixture(rel)).unwrap()
}

fn env(os: Os, arch: Arch) -> Env {
    Env {
        os,
        arch,
        os_version: if os == Os::Windows { "10.0" } else { "" }.to_owned(),
        features: Vec::new(),
    }
}

const MAC: (Os, Arch) = (Os::MacOs, Arch::Arm64);
const MAC_INTEL: (Os, Arch) = (Os::MacOs, Arch::X86_64);
const LINUX: (Os, Arch) = (Os::Linux, Arch::X86_64);
const WIN32: (Os, Arch) = (Os::Windows, Arch::X86);

fn instance() -> Instance {
    Instance {
        id: "Survival".into(),
        name: "Survival".into(),
        version: "1.21.8".into(),
        path: PathBuf::from("/data/instances/Survival"),
        ..Instance::default()
    }
}

fn steve() -> Identity {
    Identity::online(
        "Steve",
        "5627dd98e6be3c21b8a8e92344183641",
        Secret::new("token123"),
        None,
    )
}

/// Plans `version` for [`instance`] on `platform`, with default settings.
fn plan_on(version: &VersionJson, platform: (Os, Arch), identity: &Identity) -> Plan {
    plan_with(version, platform, identity, &instance(), &Config::default())
}

fn plan_with(
    version: &VersionJson,
    (os, arch): (Os, Arch),
    identity: &Identity,
    instance: &Instance,
    config: &Config,
) -> Plan {
    try_plan(version, (os, arch), identity, instance, config).unwrap()
}

fn try_plan(
    version: &VersionJson,
    (os, arch): (Os, Arch),
    identity: &Identity,
    instance: &Instance,
    config: &Config,
) -> Result<Plan, Error> {
    launch::plan(&PlanInputs {
        version,
        instance,
        config,
        data: &DataDir::new("/data"),
        java: Path::new("/java/bin/java"),
        identity,
        env: &env(os, arch),
    })
}

/// The file names on the classpath, in order.
fn classpath_names(plan: &Plan) -> Vec<String> {
    plan.classpath
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn native_names(plan: &Plan) -> Vec<String> {
    plan.natives
        .iter()
        .map(|n| n.jar.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// The value after `flag` in `args`.
fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|&a| a.to_owned()).collect()
}

const NATIVES: &str = "/data/instances/Survival/natives";
const GAME_DIR: &str = "/data/instances/Survival/.minecraft";
const CLIENT_1_21_8: &str =
    "/data/libraries/com/mojang/minecraft/1.21.8/minecraft-1.21.8-client.jar";

#[test]
fn modern_release_on_apple_silicon() {
    let plan = plan_on(&version("mojang/1.21.8.json"), MAC, &steve());
    let classpath = plan
        .classpath
        .iter()
        .map(|p| p.to_str().unwrap())
        .collect::<Vec<_>>()
        .join(":");
    assert_eq!(
        plan.jvm_args,
        strings(&[
            "-XstartOnFirstThread",
            &format!("-Djava.library.path={NATIVES}"),
            &format!("-Djna.tmpdir={NATIVES}"),
            &format!("-Dorg.lwjgl.system.SharedLibraryExtractPath={NATIVES}"),
            &format!("-Dio.netty.native.workdir={NATIVES}"),
            "-Dminecraft.launcher.brand=mctui",
            &format!("-Dminecraft.launcher.version={LAUNCHER_VERSION}"),
            "-cp",
            &classpath,
            "-Xmx2G",
            "-Xms512M",
            "-Dlog4j.configurationFile=/data/assets/log_configs/client-1.21.2.xml",
        ])
    );
    assert_eq!(plan.main_class, "net.minecraft.client.main.Main");
    assert_eq!(
        plan.game_args,
        strings(&[
            "--username",
            "Steve",
            "--version",
            "1.21.8",
            "--gameDir",
            GAME_DIR,
            "--assetsDir",
            "/data/assets",
            "--assetIndex",
            "26",
            "--uuid",
            "5627dd98e6be3c21b8a8e92344183641",
            "--accessToken",
            "token123",
            "--clientId",
            "",
            "--xuid",
            "",
            "--userType",
            "msa",
            "--versionType",
            "release",
        ])
    );
    assert_eq!(plan.working_dir, Path::new(GAME_DIR));
    assert_eq!(plan.java, Path::new("/java/bin/java"));
    assert_eq!(plan.natives_dir, Path::new(NATIVES));
    assert!(plan.natives.is_empty(), "LWJGL 3.3 unpacks its own");

    // The game's jar last, in the Go app's place; natives jars by classifier.
    assert_eq!(plan.classpath.last().unwrap(), Path::new(CLIENT_1_21_8));
    let names = classpath_names(&plan);
    assert!(names.contains(&"lwjgl-3.3.3-natives-macos-arm64.jar".to_owned()));
    assert!(names.contains(&"java-objc-bridge-1.1.jar".to_owned()));
    assert!(!names.iter().any(|n| n.contains("natives-linux")));

    // Downloads: every classpath jar, then the logging setup; assets apart.
    let client = plan
        .files
        .iter()
        .find(|f| f.dest == Path::new(CLIENT_1_21_8))
        .unwrap();
    assert_eq!(
        client.hash,
        Some(Hash::Sha1(
            "a19d9badbea944a4369fd0059e53bf7286597576".into()
        ))
    );
    assert_eq!(plan.files.len(), plan.classpath.len() + 1);
    let logging = plan.files.last().unwrap();
    assert_eq!(
        logging.dest,
        Path::new("/data/assets/log_configs/client-1.21.2.xml")
    );
    let assets = plan.assets.as_ref().unwrap();
    assert_eq!(assets.index.dest, Path::new("/data/assets/indexes/26.json"));
    assert_eq!(assets.objects_dir, Path::new("/data/assets/objects"));
    assert_eq!(assets.objects_url, launch::RESOURCES_URL);
    assert_eq!(assets.layout, AssetLayout::Hashed);
}

#[test]
fn rules_pick_arguments_and_libraries_per_platform() {
    let v = version("mojang/1.21.8.json");
    let linux = plan_on(&v, LINUX, &steve());
    assert!(!linux.jvm_args.contains(&"-XstartOnFirstThread".to_owned()));
    let names = classpath_names(&linux);
    assert!(names.contains(&"lwjgl-3.3.3-natives-linux.jar".to_owned()));
    assert!(
        !names
            .iter()
            .any(|n| n.contains("objc") || n.contains("macos"))
    );
    assert!(
        value_of(&linux.jvm_args, "-cp")
            .unwrap()
            .contains(".jar:/data/")
    );

    let windows = plan_on(&v, WIN32, &steve());
    let jvm = &windows.jvm_args;
    assert!(jvm.contains(&"-Xss1M".to_owned()));
    assert!(jvm.iter().any(|a| a.starts_with("-XX:HeapDumpPath=")));
    assert!(!jvm.contains(&"-XstartOnFirstThread".to_owned()));
    assert!(
        value_of(jvm, "-cp").unwrap().contains(".jar;/data/"),
        "; on Windows"
    );
    let names = classpath_names(&windows);
    assert!(names.contains(&"lwjgl-3.3.3-natives-windows-x86.jar".to_owned()));

    // Features stay off: no demo, window size, or quick play arguments.
    for plan in [&linux, &windows] {
        for arg in [
            "--demo",
            "--width",
            "--quickPlayPath",
            "--quickPlayMultiplayer",
        ] {
            assert!(!plan.game_args.contains(&arg.to_owned()), "{arg}");
        }
    }
}

#[test]
fn natives_by_classifier_and_duplicates_listed_once() {
    let v = version("mojang/1.16.5.json");
    let mac = plan_on(&v, MAC_INTEL, &steve());
    assert_eq!(
        native_names(&mac),
        [
            "lwjgl-3.2.1-natives-macos.jar",
            "lwjgl-glfw-3.2.1-natives-macos.jar",
            "java-objc-bridge-1.0.0-natives-osx.jar",
        ]
    );
    let bridge = mac.natives.last().unwrap();
    assert_eq!(bridge.exclude, ["META-INF/"]);
    assert!(
        bridge
            .jar
            .starts_with("/data/libraries/ca/weblite/java-objc-bridge/1.0.0")
    );
    // LWJGL 3.2.1 is listed twice, the bridge twice: each on the classpath
    // and downloaded once.
    assert_eq!(
        classpath_names(&mac),
        [
            "patchy-1.3.9.jar",
            "oshi-core-1.1.jar",
            "jna-4.4.0.jar",
            "lwjgl-3.2.1.jar",
            "lwjgl-glfw-3.2.1.jar",
            "text2speech-1.11.3.jar",
            "java-objc-bridge-1.0.0.jar",
            "minecraft-1.16.5-client.jar",
        ]
    );
    assert_eq!(mac.files.len(), mac.classpath.len() + mac.natives.len() + 1);
    for native in &mac.natives {
        assert!(
            mac.files.iter().any(|f| f.dest == native.jar),
            "natives are downloaded"
        );
    }

    let linux = plan_on(&v, LINUX, &steve());
    assert_eq!(
        native_names(&linux),
        [
            "lwjgl-3.2.2-natives-linux.jar",
            "lwjgl-glfw-3.2.2-natives-linux.jar",
            "text2speech-1.11.3-natives-linux.jar",
        ]
    );
    assert!(classpath_names(&linux).contains(&"lwjgl-3.2.2.jar".to_owned()));
    // 1.16.5 has JVM arguments, so -XstartOnFirstThread comes from its rules.
    assert_eq!(mac.jvm_args[0], "-XstartOnFirstThread");
    assert_ne!(linux.jvm_args[0], "-XstartOnFirstThread");
}

#[test]
fn legacy_version_offline() {
    let v = version("mojang/1.8.9.json");
    let player = Identity::offline("");
    let mut inst = instance();
    inst.version = "1.8.9".into();
    let plan = plan_with(&v, MAC_INTEL, &player, &inst, &Config::default());
    let classpath = value_of(&plan.jvm_args, "-cp").unwrap().to_owned();
    assert_eq!(
        plan.jvm_args,
        strings(&[
            &format!("-Djava.library.path={NATIVES}"),
            "-Dminecraft.launcher.brand=mctui",
            &format!("-Dminecraft.launcher.version={LAUNCHER_VERSION}"),
            "-cp",
            &classpath,
            "-Xmx2G",
            "-Xms512M",
            "-Dlog4j.configurationFile=/data/assets/log_configs/client-1.7.xml",
        ]),
        "no -XstartOnFirstThread: LWJGL 2 hangs with it"
    );
    let uuid = launch::offline_uuid("Player");
    assert_eq!(
        plan.game_args,
        strings(&[
            "--username",
            "Player",
            "--version",
            "1.8.9",
            "--gameDir",
            GAME_DIR,
            "--assetsDir",
            "/data/assets",
            "--assetIndex",
            "1.8",
            "--uuid",
            &uuid,
            "--accessToken",
            "0",
            "--userProperties",
            "{}",
            "--userType",
            "legacy",
        ])
    );
    assert_eq!(
        native_names(&plan),
        [
            "lwjgl-platform-2.9.2-nightly-20140822-natives-osx.jar",
            "jinput-platform-2.0.5-natives-osx.jar",
            "twitch-platform-6.5-natives-osx.jar",
        ]
    );
    assert!(plan.natives.iter().all(|n| n.exclude == ["META-INF/"]));
    assert_eq!(
        classpath_names(&plan),
        [
            "netty-1.8.8.jar",
            "oshi-core-1.1.jar",
            "jna-3.4.0.jar",
            "lwjgl-2.9.2-nightly-20140822.jar",
            "lwjgl_util-2.9.2-nightly-20140822.jar",
            "minecraft-1.8.9-client.jar",
        ]
    );

    let windows = plan_with(&v, WIN32, &player, &inst, &Config::default());
    assert!(
        native_names(&windows).contains(&"twitch-platform-6.5-natives-windows-32.jar".to_owned())
    );
}

#[test]
fn snapshot_with_natives_subfolders_and_suggested_jvm_options() {
    let v = version("mojang/26.4-snapshot-1.json");
    let plan = plan_on(&v, MAC, &steve());
    let jvm = &plan.jvm_args;
    assert!(jvm.contains(&format!("-Djava.library.path={NATIVES}/java")));
    assert!(jvm.contains(&format!(
        "-Dorg.lwjgl.system.SharedLibraryExtractPath={NATIVES}/lwjgl"
    )));
    assert!(jvm.contains(&"--enable-native-access=ALL-UNNAMED".to_owned()));
    assert_eq!(value_of(&plan.game_args, "--versionType"), Some("snapshot"));
    assert_eq!(
        value_of(&plan.game_args, "--userType"),
        None,
        "gone in 26.x"
    );
    assert_eq!(value_of(&plan.game_args, "--assetIndex"), Some("35"));
    // The player's own JVM options replace the suggested ones...
    assert!(!jvm.contains(&"-XX:+UseZGC".to_owned()));
    // ...which are used without any.
    let bare = Config {
        jvm_args: Vec::new(),
        ..Config::default()
    };
    let plan = plan_with(&v, MAC, &steve(), &instance(), &bare);
    let after_cp = plan.jvm_args.iter().skip_while(|a| *a != "-cp").skip(2);
    let user: Vec<&str> = after_cp.map(String::as_str).collect();
    assert_eq!(
        user,
        [
            "-Xms2G",
            "-Xmx4G",
            "-XX:+UseCompactObjectHeaders",
            "-XX:+AlwaysPreTouch",
            "-XX:+UseStringDeduplication",
            "-XX:+UseZGC",
            "-Dlog4j.configurationFile=/data/assets/log_configs/client-1.21.2.xml",
        ]
    );
}

#[test]
fn fabric_merged_version() {
    let merged = fabric::merge(
        version("mojang/1.21.8.json"),
        version("fabric/profile_1.21.8_0.19.5.json"),
    );
    let plan = plan_on(&merged, LINUX, &steve());
    assert_eq!(
        plan.main_class,
        "net.fabricmc.loader.impl.launch.knot.KnotClient"
    );
    let cp = plan.jvm_args.iter().position(|a| a == "-cp").unwrap();
    assert_eq!(
        plan.jvm_args[cp + 2],
        "-DFabricMcEmu= net.minecraft.client.main.Main ",
        "Fabric's JVM arguments follow the game's"
    );
    let names = classpath_names(&plan);
    assert!(names.contains(&"asm-9.10.1.jar".to_owned()));
    assert!(
        !names.contains(&"asm-9.6.jar".to_owned()),
        "replaced by Fabric's"
    );
    assert!(names.contains(&"fabric-loader-0.19.5.jar".to_owned()));
    assert_eq!(names.last().unwrap(), "minecraft-1.21.8-client.jar");
    assert_eq!(value_of(&plan.game_args, "--version"), Some("1.21.8"));

    let loader = plan
        .files
        .iter()
        .find(|f| f.dest.ends_with("fabric-loader-0.19.5.jar"))
        .unwrap();
    assert_eq!(
        loader.url,
        "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar"
    );
    assert_eq!(loader.hash, None, "Fabric publishes no checksum for it");
}

#[test]
fn instance_memory_replaces_only_the_global_memory() {
    let mut inst = instance();
    inst.jvm_args = strings(&["-Xmx6G", "-Dfoo=bar"]);
    let plan = plan_with(
        &version("mojang/1.21.8.json"),
        LINUX,
        &steve(),
        &inst,
        &Config::default(),
    );
    let tail: Vec<&str> = plan
        .jvm_args
        .iter()
        .rev()
        .take(4)
        .rev()
        .map(String::as_str)
        .collect();
    assert_eq!(
        tail,
        [
            "-Xms512M",
            "-Xmx6G",
            "-Dfoo=bar",
            "-Dlog4j.configurationFile=/data/assets/log_configs/client-1.21.2.xml"
        ]
    );
}

/// A small version JSON, for what the real ones don't show.
fn synthetic(extra: &serde_json::Value) -> VersionJson {
    let mut json = serde_json::json!({
        "id": "test-1",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "downloads": {"client": {"sha1": "00", "size": 1, "url": "https://x/client.jar"}},
        "libraries": [],
    });
    json.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(json).unwrap()
}

fn lib(name: &str, path: &str) -> serde_json::Value {
    serde_json::json!({"name": name, "downloads": {"artifact":
        {"path": path, "sha1": "ab", "size": 2, "url": format!("https://x/{path}")}}})
}

#[test]
fn placeholders_are_filled_once() {
    let v = synthetic(&serde_json::json!({
        "minecraftArguments": "--username ${auth_player_name} --session ${auth_session} --quick ${quickPlayPath} --assets ${game_assets}",
    }));
    // A name that looks like a placeholder stays a name (L-14).
    let sneaky = Identity::offline("${auth_access_token}");
    let plan = plan_on(&v, LINUX, &sneaky);
    let uuid = launch::offline_uuid("${auth_access_token}");
    assert_eq!(
        plan.game_args,
        strings(&[
            "--username",
            "${auth_access_token}",
            "--session",
            &format!("token:0:{uuid}"),
            "--quick",
            "${quickPlayPath}",
            "--assets",
            "/data/assets",
        ])
    );
}

#[test]
fn old_asset_layouts() {
    let index = |id: &str| {
        serde_json::json!({
            "assetIndex": {"id": id, "sha1": "00", "size": 1, "totalSize": 1, "url": "https://x/i.json"},
            "minecraftArguments": "--assetsDir ${game_assets} --assetIndex ${assets_index_name}",
        })
    };
    let virtual_plan = plan_on(&synthetic(&index("legacy")), LINUX, &steve());
    let dir = PathBuf::from("/data/assets/virtual/legacy");
    assert_eq!(
        virtual_plan.assets.unwrap().layout,
        AssetLayout::Virtual(dir)
    );
    assert_eq!(
        virtual_plan.game_args,
        [
            "--assetsDir",
            "/data/assets/virtual/legacy",
            "--assetIndex",
            "legacy"
        ]
    );

    let resources = plan_on(&synthetic(&index("pre-1.6")), LINUX, &steve());
    let dir = PathBuf::from(GAME_DIR).join("resources");
    assert_eq!(
        resources.assets.unwrap().layout,
        AssetLayout::Resources(dir)
    );
    assert_eq!(
        value_of(&resources.game_args, "--assetsDir"),
        Some(&*format!("{GAME_DIR}/resources"))
    );
}

#[test]
fn unsafe_paths_are_refused() {
    let evil_library = synthetic(&serde_json::json!({
        "libraries": [lib("g:a:1", "../../../etc/evil.jar")],
    }));
    let err = try_plan(
        &evil_library,
        LINUX,
        &steve(),
        &instance(),
        &Config::default(),
    );
    assert!(matches!(err, Err(Error::UnsafePath(p)) if p == "../../../etc/evil.jar"));

    let mut evil_id = synthetic(&serde_json::json!({}));
    evil_id.id = "../x".into();
    let err = try_plan(&evil_id, LINUX, &steve(), &instance(), &Config::default());
    assert!(matches!(err, Err(Error::UnsafePath(_))));

    let no_client = VersionJson {
        downloads: None,
        ..synthetic(&serde_json::json!({}))
    };
    let err = try_plan(&no_client, LINUX, &steve(), &instance(), &Config::default());
    assert!(matches!(err, Err(Error::NoClient(id)) if id == "test-1"));
}

#[test]
fn download_mark_needs_a_matching_key() {
    let v = version("mojang/1.21.8.json");
    let mut inst = instance();
    let plan = |inst: &Instance| plan_with(&v, LINUX, &steve(), inst, &Config::default());
    assert!(!plan(&inst).downloaded);
    inst.is_fully_downloaded = true;
    inst.download_cache_key = inst.download_key();
    assert!(plan(&inst).downloaded);
    assert_eq!(plan(&inst).download_key, "1.21.8|vanilla|");
    inst.loader = Loader::Fabric;
    assert!(!plan(&inst).downloaded, "the loader changed since");
}

#[test]
fn debug_output_hides_the_token() {
    let plan = plan_on(&version("mojang/1.21.8.json"), LINUX, &steve());
    assert!(plan.game_args.contains(&"token123".to_owned()));
    let debug = format!("{plan:?}");
    assert!(!debug.contains("token123"), "{debug}");
    assert!(debug.contains("<redacted>"));
    // Nor does the command that starts the game.
    let debug = format!("{:?}", plan.command());
    assert!(!debug.contains("token123"), "{debug}");
    assert!(debug.contains("<redacted>"), "{debug}");
    let args: Vec<&str> = plan.args().collect();
    assert_eq!(args.len(), plan.jvm_args.len() + 1 + plan.game_args.len());
    assert_eq!(args[plan.jvm_args.len()], plan.main_class);
}

/// Collects every step a launch reports.
#[derive(Default)]
struct Steps(Mutex<Vec<Step>>);

impl Steps {
    fn push(&self, step: Step) {
        self.0.lock().unwrap().push(step);
    }

    fn all(&self) -> Vec<Step> {
        self.0.lock().unwrap().clone()
    }

    /// Each stage once, in the order first seen.
    fn stages(&self) -> Vec<launch::Stage> {
        let mut stages = Vec::new();
        for step in self.all() {
            if stages.last() != Some(&step.stage()) {
                stages.push(step.stage());
            }
        }
        stages
    }
}

mod prepare {
    //! Preparing a synthetic version against a mock server.

    use std::io::Write as _;
    use std::time::Duration;

    use mctui_core::launch::Stage;
    use mctui_core::net::{Http, Progress};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    pub fn sha1(bytes: &[u8]) -> String {
        use sha1::Digest as _;
        use std::fmt::Write as _;
        sha1::Sha1::digest(bytes)
            .iter()
            .fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            })
    }

    /// A natives jar with a library and a manifest to leave out.
    pub fn natives_jar() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("META-INF/MANIFEST.MF", "Manifest-Version: 1.0"),
            ("liblwjgl.so", "native code"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    pub const SOUND: &[u8] = b"a sound";
    pub const ICON: &[u8] = b"an icon";

    /// Serves a tiny game: a client jar, a library, a natives jar, a library
    /// with no checksum (as some of Fabric's), a logging setup, and an asset
    /// index of two objects under three names.
    pub struct Game {
        pub server: MockServer,
        pub version: VersionJson,
        /// The version JSON as served.
        pub raw: Vec<u8>,
    }

    impl Game {
        pub async fn start(index_id: &str) -> Self {
            Self::start_with(index_id, None).await
        }

        /// With the library's response held back by `delay`.
        pub async fn start_with(index_id: &str, delay: Option<Duration>) -> Self {
            let server = MockServer::start().await;
            let uri = server.uri();
            let index = json!({"objects": {
                "minecraft/sounds/a.ogg": {"hash": sha1(SOUND), "size": SOUND.len()},
                "icons/b.png": {"hash": sha1(ICON), "size": ICON.len()},
                "icons/copy-of-a.ogg": {"hash": sha1(SOUND), "size": SOUND.len()},
            }})
            .to_string();
            let natives = natives_jar();
            let files: Vec<(String, Vec<u8>)> = vec![
                ("/client.jar".into(), b"the game".to_vec()),
                ("/lib.jar".into(), b"a library".to_vec()),
                ("/natives.jar".into(), natives.clone()),
                (
                    "/maven/com/example/nosum/1/nosum-1.jar".into(),
                    b"no checksum".to_vec(),
                ),
                ("/log.xml".into(), b"<Configuration/>".to_vec()),
                ("/index.json".into(), index.clone().into_bytes()),
                (object_route(SOUND), SOUND.to_vec()),
                (object_route(ICON), ICON.to_vec()),
            ];
            for (route, body) in &files {
                let mut response = ResponseTemplate::new(200).set_body_bytes(body.clone());
                if let (Some(delay), "/lib.jar") = (delay, route.as_str()) {
                    response = response.set_delay(delay);
                }
                Mock::given(method("GET"))
                    .and(path(route.as_str()))
                    .respond_with(response)
                    .mount(&server)
                    .await;
            }
            let remote = |route: &str, body: &[u8]| json!({"sha1": sha1(body), "size": body.len(), "url": format!("{uri}{route}")});
            let native = remote("/natives.jar", &natives);
            let mut native_download = native.clone();
            native_download["path"] =
                json!("org/lwjgl/lwjgl-platform/2/lwjgl-platform-2-natives.jar");
            let mut lib = remote("/lib.jar", b"a library");
            lib["path"] = json!("com/example/lib/1/lib-1.jar");
            let version = json!({
                "id": "test-1",
                "type": "release",
                "mainClass": "net.minecraft.client.main.Main",
                "minecraftArguments": "--username ${auth_player_name} --accessToken ${auth_access_token} --assetsDir ${game_assets}",
                "assets": index_id,
                "assetIndex": {
                    "id": index_id, "sha1": sha1(index.as_bytes()), "size": index.len(),
                    "totalSize": SOUND.len() + ICON.len(), "url": format!("{uri}/index.json")
                },
                "downloads": {"client": remote("/client.jar", b"the game")},
                "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21},
                "libraries": [
                    {"name": "com.example:lib:1", "downloads": {"artifact": lib}},
                    {
                        "name": "org.lwjgl:lwjgl-platform:2",
                        "natives": {"linux": "natives", "osx": "natives", "windows": "natives"},
                        "extract": {"exclude": ["META-INF/"]},
                        "downloads": {"classifiers": {"natives": native_download}},
                    },
                    {"name": "com.example:nosum:1", "url": format!("{uri}/maven/")},
                ],
                "logging": {"client": {
                    "argument": "-Dlog4j.configurationFile=${path}",
                    "file": {"id": "client-test.xml", "sha1": sha1(b"<Configuration/>"), "size": 16, "url": format!("{uri}/log.xml")},
                    "type": "log4j2-xml",
                }},
            });
            let raw = serde_json::to_vec(&version).unwrap();
            Self {
                version: serde_json::from_slice(&raw).unwrap(),
                raw,
                server,
            }
        }

        pub fn plan(&self, data: &DataDir, instance: &Instance) -> Plan {
            let mut plan = launch::plan(&PlanInputs {
                version: &self.version,
                instance,
                config: &Config::default(),
                data,
                java: Path::new("java"),
                identity: &Identity::offline("Steve"),
                env: &Env::current(),
            })
            .unwrap();
            plan.assets.as_mut().unwrap().objects_url = format!("{}/objects", self.server.uri());
            plan
        }

        /// Requests so far, by path.
        pub async fn requests(&self) -> Vec<String> {
            let requests = self.server.received_requests().await.unwrap_or_default();
            requests.iter().map(|r| r.url.path().to_owned()).collect()
        }
    }

    pub fn object_route(body: &[u8]) -> String {
        let hash = sha1(body);
        format!("/objects/{}/{hash}", &hash[..2])
    }

    /// A data dir and an instance folder in a temp dir.
    pub fn temp_instance(root: &Path) -> (DataDir, Instance) {
        let data = DataDir::new(root.join("data"));
        let instance = Instance {
            id: "Test".into(),
            name: "Test".into(),
            version: "test-1".into(),
            path: data.instances_dir().join("Test"),
            ..Instance::default()
        };
        std::fs::create_dir_all(&instance.path).unwrap();
        (data, instance)
    }

    /// Every file under `dir`, relative, with `/` separators.
    pub fn files_under(dir: &Path) -> Vec<String> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(dir, &mut files);
        let mut names: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        names.sort();
        names
    }

    fn last_progress(steps: &[Step], stage: Stage) -> Progress {
        steps
            .iter()
            .rev()
            .find_map(|step| match (step.stage() == stage, step) {
                (true, Step::Libraries(p) | Step::Assets(p)) => Some(*p),
                _ => None,
            })
            .unwrap()
    }

    #[tokio::test]
    async fn fetches_verifies_and_unpacks_everything_once() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, instance) = temp_instance(tmp.path());
        let plan = game.plan(&data, &instance);
        let steps = Steps::default();

        let prepared = launch::prepare(&Http::new().unwrap(), &plan, |s| steps.push(s))
            .await
            .unwrap();
        assert!(prepared.verified);
        assert_eq!(
            steps.stages(),
            [Stage::Libraries, Stage::Assets, Stage::Natives]
        );
        let all = steps.all();
        let libraries = last_progress(&all, Stage::Libraries);
        assert_eq!((libraries.files_done, libraries.files_total), (5, 5));
        let first = all.first().unwrap();
        assert!(
            matches!(first, Step::Libraries(p) if p.files_total == 5 && p.files_done == 0),
            "totals come first: {first:?}"
        );
        let assets = last_progress(&all, Stage::Assets);
        assert_eq!((assets.files_done, assets.files_total), (2, 2));
        assert_eq!(assets.bytes_done, (SOUND.len() + ICON.len()) as u64);

        let sound = sha1(SOUND);
        let icon = sha1(ICON);
        assert_eq!(
            files_under(data.root()),
            [
                "assets/indexes/test.json".to_owned(),
                "assets/log_configs/client-test.xml".into(),
                format!("assets/objects/{}/{icon}", &icon[..2]),
                format!("assets/objects/{}/{sound}", &sound[..2]),
                "instances/Test/natives/.mctui-natives".into(),
                "instances/Test/natives/liblwjgl.so".into(),
                "libraries/com/example/lib/1/lib-1.jar".into(),
                "libraries/com/example/nosum/1/nosum-1.jar".into(),
                "libraries/com/mojang/minecraft/test-1/minecraft-test-1-client.jar".into(),
                "libraries/org/lwjgl/lwjgl-platform/2/lwjgl-platform-2-natives.jar".into(),
            ]
        );
        assert!(plan.working_dir.is_dir());
        // An object listed under two names is fetched once (L-5).
        let requests = game.requests().await;
        let fetched = |route: &str| requests.iter().filter(|r| *r == route).count();
        assert_eq!(fetched(&object_route(SOUND)), 1);
        assert_eq!(requests.len(), 8, "{requests:?}");
    }

    #[tokio::test]
    async fn a_downloaded_instance_only_checks_its_files() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, mut instance) = temp_instance(tmp.path());
        let http = Http::new().unwrap();
        let first = game.plan(&data, &instance);
        launch::prepare(&http, &first, |_| {}).await.unwrap();
        let fetched = game.requests().await.len();

        instance.is_fully_downloaded = true;
        instance.download_cache_key = instance.download_key();
        let plan = game.plan(&data, &instance);
        assert!(plan.downloaded);
        let steps = Steps::default();
        let prepared = launch::prepare(&http, &plan, |s| steps.push(s))
            .await
            .unwrap();
        assert!(!prepared.verified, "nothing checked against checksums");
        assert_eq!(game.requests().await.len(), fetched, "no network at all");
        assert_eq!(
            steps.stages(),
            [Stage::Libraries, Stage::Assets, Stage::Natives]
        );
        let libraries = last_progress(&steps.all(), Stage::Libraries);
        assert_eq!((libraries.files_done, libraries.files_total), (5, 5));

        // A file gone: everything is verified again, and only it fetched.
        let lib = data.libraries_dir().join("com/example/lib/1/lib-1.jar");
        std::fs::remove_file(&lib).unwrap();
        let prepared = launch::prepare(&http, &plan, |_| {}).await.unwrap();
        assert!(prepared.verified);
        assert_eq!(std::fs::read(&lib).unwrap(), b"a library");
        let requests = game.requests().await;
        assert_eq!(requests[fetched..], ["/lib.jar"]);

        // A missing asset object is noticed too.
        let icon = sha1(ICON);
        std::fs::remove_file(
            data.assets_dir()
                .join("objects")
                .join(&icon[..2])
                .join(&icon),
        )
        .unwrap();
        assert!(
            launch::prepare(&http, &plan, |_| {})
                .await
                .unwrap()
                .verified
        );
    }

    /// Mojang republishes asset indexes under the same id: a downloaded
    /// instance with the old one fetches the new one and its objects.
    #[tokio::test]
    async fn a_downloaded_instance_notices_a_republished_asset_index() {
        const MUSIC: &[u8] = b"new music";
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, mut instance) = temp_instance(tmp.path());
        let http = Http::new().unwrap();
        launch::prepare(&http, &game.plan(&data, &instance), |_| {})
            .await
            .unwrap();
        instance.is_fully_downloaded = true;
        instance.download_cache_key = instance.download_key();

        let index = json!({"objects": {
            "minecraft/sounds/a.ogg": {"hash": sha1(SOUND), "size": SOUND.len()},
            "minecraft/music/new.ogg": {"hash": sha1(MUSIC), "size": MUSIC.len()},
        }})
        .to_string();
        for (route, body) in [
            ("/index-v2.json".to_owned(), index.as_bytes()),
            (object_route(MUSIC), MUSIC),
        ] {
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
                .mount(&game.server)
                .await;
        }
        let mut plan = game.plan(&data, &instance);
        assert!(plan.downloaded);
        let assets = plan.assets.as_mut().unwrap();
        assets.index.url = format!("{}/index-v2.json", game.server.uri());
        assets.index.hash = Some(Hash::Sha1(sha1(index.as_bytes())));
        assets.index.size = Some(index.len() as u64);

        let prepared = launch::prepare(&http, &plan, |_| {}).await.unwrap();
        assert!(prepared.verified, "checked and fetched again");
        let indexes = data.assets_dir().join("indexes/test.json");
        assert_eq!(std::fs::read_to_string(indexes).unwrap(), index);
        let music = sha1(MUSIC);
        let object = data
            .assets_dir()
            .join("objects")
            .join(&music[..2])
            .join(&music);
        assert_eq!(std::fs::read(object).unwrap(), MUSIC);
    }

    #[tokio::test]
    async fn old_versions_get_assets_by_name() {
        let game = Game::start("legacy").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, instance) = temp_instance(tmp.path());
        let plan = game.plan(&data, &instance);
        launch::prepare(&Http::new().unwrap(), &plan, |_| {})
            .await
            .unwrap();
        let virtual_dir = data.assets_dir().join("virtual/legacy");
        assert_eq!(
            files_under(&virtual_dir),
            [
                "icons/b.png",
                "icons/copy-of-a.ogg",
                "minecraft/sounds/a.ogg"
            ]
        );
        assert_eq!(
            std::fs::read(virtual_dir.join("icons/copy-of-a.ogg")).unwrap(),
            SOUND
        );
        assert_eq!(
            value_of(&plan.game_args, "--assetsDir"),
            virtual_dir.to_str()
        );

        let game = Game::start("pre-1.6").await;
        let plan = game.plan(&data, &instance);
        launch::prepare(&Http::new().unwrap(), &plan, |_| {})
            .await
            .unwrap();
        let resources = instance.game_dir().join("resources");
        assert_eq!(
            files_under(&resources),
            [
                "icons/b.png",
                "icons/copy-of-a.ogg",
                "minecraft/sounds/a.ogg"
            ]
        );
    }

    #[tokio::test]
    async fn cancelling_leaves_no_partial_files() {
        let game = Game::start_with("test", Some(Duration::from_secs(30))).await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, instance) = temp_instance(tmp.path());
        let plan = game.plan(&data, &instance);
        let http = Http::new().unwrap();
        let cancelled = tokio::time::timeout(
            Duration::from_millis(500),
            launch::prepare(&http, &plan, |_| {}),
        )
        .await;
        assert!(cancelled.is_err(), "still waiting on the library");
        let left = files_under(data.root());
        assert!(
            !left
                .iter()
                .any(|f| f.contains("lib-1.jar") || f.contains(".tmp")),
            "{left:?}"
        );
        assert!(!plan.natives_dir.exists());
    }

    #[tokio::test]
    async fn a_damaged_asset_index_is_an_error() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, instance) = temp_instance(tmp.path());
        let mut plan = game.plan(&data, &instance);
        // The index itself downloads fine, but lists an object with a
        // hash that would make a path.
        let evil = json!({"objects": {"x": {"hash": "../../../../etc/passwd", "size": 1}}});
        let body = evil.to_string().into_bytes();
        Mock::given(method("GET"))
            .and(path("/evil-index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&game.server)
            .await;
        let assets = plan.assets.as_mut().unwrap();
        assets.index.url = format!("{}/evil-index.json", game.server.uri());
        assets.index.hash = Some(Hash::Sha1(sha1(&body)));
        let err = launch::prepare(&Http::new().unwrap(), &plan, |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::AssetIndex { .. }), "{err}");
    }
}

#[cfg(unix)]
mod whole_launch {
    //! [`Launcher::launch`] end to end, with a mock Mojang and a shell
    //! script standing in for Java.

    use std::os::unix::fs::PermissionsExt as _;
    use std::time::Duration;

    use mctui_core::java::{Runtimes, Source, StaleJava, Staleness};
    use mctui_core::launch::{Exit, Launcher, Stage};
    use mctui_core::modrinth;
    use mctui_core::mojang::Mojang;
    use mctui_core::net::Http;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::prepare::{Game, sha1, temp_instance};
    use super::*;

    /// Answers `-version` as Java 21, and otherwise prints where it runs and
    /// its arguments, one per line.
    const FAKE_JAVA: &str = r#"#!/bin/sh
if [ "$1" = "-version" ]; then echo 'openjdk version "21.0.2" 2024-01-16' >&2; exit 0; fi
echo "cwd: $(pwd -P)"
for arg in "$@"; do echo "arg: $arg"; done
"#;

    /// A Java 21 home at `home` built for `os_arch` (as Java names it),
    /// running [`FAKE_JAVA`]. Returns its `java`.
    fn fake_jdk(home: &Path, os_arch: &str) -> PathBuf {
        let java = home.join("bin/java");
        std::fs::create_dir_all(java.parent().unwrap()).unwrap();
        std::fs::write(&java, FAKE_JAVA).unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        let release = format!("JAVA_VERSION=\"21.0.2\"\nOS_ARCH=\"{os_arch}\"\n");
        std::fs::write(home.join("release"), release).unwrap();
        java
    }

    /// An Intel [`fake_jdk`] in `dir`: the test version only has Intel
    /// natives for macOS, so on Apple silicon only an Intel Java runs it.
    fn fake_java(dir: &Path) -> PathBuf {
        fake_jdk(&dir.join("jdk"), "x86_64")
    }

    async fn launcher(game: &Game, data: &DataDir) -> Launcher {
        let uri = game.server.uri();
        let manifest = json!({
            "latest": {"release": "test-1", "snapshot": "test-1"},
            "versions": [{
                "id": "test-1", "type": "release", "url": format!("{uri}/v/test-1.json"),
                "sha1": sha1(&game.raw), "releaseTime": "2026-01-01T00:00:00Z"
            }],
        });
        Mock::given(method("GET"))
            .and(path("/manifest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(manifest))
            .mount(&game.server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v/test-1.json"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(game.raw.clone()))
            .mount(&game.server)
            .await;
        let http = Http::new().unwrap();
        let mut launcher = Launcher::new(
            http.clone(),
            data.clone(),
            Runtimes {
                managed: data.root().join("no-runtimes"),
                go_managed: None,
                java_home: None,
                path: Vec::new(),
                roots: Vec::new(),
                adoptium_url: uri.clone(),
                os: Os::current(),
                arch: Arch::current(),
            },
        );
        launcher.mojang =
            Mojang::with_manifest_url(http.clone(), data, format!("{uri}/manifest.json"));
        launcher.fabric =
            fabric::Fabric::with_meta_url(http.clone(), data, format!("{uri}/fabric"));
        launcher.modrinth = modrinth::Client::with_base_url(http, format!("{uri}/modrinth"));
        launcher.resources_url = format!("{uri}/objects");
        launcher.stop_grace = Duration::from_secs(2);
        launcher
    }

    async fn all_lines(logs: &mut launch::Logs) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(line) = logs.recv().await {
            lines.push(line.text);
        }
        lines
    }

    #[tokio::test]
    async fn launches_a_vanilla_instance() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, mut instance) = temp_instance(tmp.path());
        let launcher = launcher(&game, &data).await;
        let config = Config {
            java_path: Some(fake_java(tmp.path())),
            ..Config::default()
        };
        let steps = Steps::default();

        let mut started = launcher
            .launch(&instance, &config, &Identity::offline("Steve"), |s| {
                steps.push(s);
            })
            .await
            .unwrap();
        assert_eq!(
            steps.stages(),
            [
                Stage::Version,
                Stage::Java,
                Stage::Libraries,
                Stage::Assets,
                Stage::Natives,
                Stage::Starting
            ]
        );
        assert_eq!(started.java.source, Source::Explicit);
        assert!(!started.stale);
        assert!(started.starter_mods.is_none());

        let lines = all_lines(&mut started.logs).await;
        let report = started.game.wait().await;
        assert_eq!(report.exit, Exit::Code(0));
        assert!(!report.crashed());
        let game_dir = std::fs::canonicalize(instance.game_dir()).unwrap();
        assert_eq!(lines[0], format!("cwd: {}", game_dir.display()));
        let args: Vec<&str> = lines[1..]
            .iter()
            .map(|l| l.strip_prefix("arg: ").unwrap())
            .collect();
        assert!(args.windows(2).any(|w| w == ["--username", "Steve"]));
        assert!(args.contains(&"net.minecraft.client.main.Main"));
        let logging = data.assets_dir().join("log_configs/client-test.xml");
        assert!(args.contains(&&*format!(
            "-Dlog4j.configurationFile={}",
            logging.display()
        )));

        // The instance is marked downloaded by the caller, from the changes.
        assert_eq!(started.changes.downloaded, Some(instance.download_key()));
        started.changes.apply(&mut instance);
        assert!(instance.is_fully_downloaded && instance.cached_at.is_some());
        report.record_play(&mut instance);
        assert!(instance.last_played.is_some());

        // Launched again: the files are only checked, and it still starts.
        let again = launcher
            .launch(&instance, &config, &Identity::offline("Steve"), |_| {})
            .await
            .unwrap();
        assert_eq!(again.changes.downloaded, None, "already marked");
        assert_eq!(again.game.wait().await.exit, Exit::Code(0));
    }

    #[tokio::test]
    async fn launches_fabric_without_a_loader_version_and_with_failing_starter_mods() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, mut instance) = temp_instance(tmp.path());
        instance.loader = Loader::Fabric;
        instance.install_starter_fabric_mods = true;
        let launcher = launcher(&game, &data).await;
        let uri = game.server.uri();
        Mock::given(method("GET"))
            .and(path("/fabric/v2/versions/loader/test-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"loader": {"version": "0.20.0-beta.1", "stable": false}},
                {"loader": {"version": "0.19.5", "stable": true}},
            ])))
            .mount(&game.server)
            .await;
        let profile = json!({
            "id": "fabric-loader-0.19.5-test-1", "inheritsFrom": "test-1", "type": "release",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {"game": [], "jvm": ["-Dfabric.test=1"]},
            "libraries": [{"name": "com.example:nosum:1", "url": format!("{uri}/maven/")}],
        });
        Mock::given(method("GET"))
            .and(path(
                "/fabric/v2/versions/loader/test-1/0.19.5/profile/json",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(profile))
            .mount(&game.server)
            .await;
        // Modrinth answers nothing: every starter mod fails.
        let config = Config {
            java_path: Some(fake_java(tmp.path())),
            ..Config::default()
        };
        let steps = Steps::default();

        let mut started = launcher
            .launch(&instance, &config, &Identity::offline("Alex"), |s| {
                steps.push(s);
            })
            .await
            .unwrap();
        assert!(steps.stages().contains(&Stage::StarterMods));
        let report = started.starter_mods.as_ref().unwrap();
        assert!(!report.all_ok());
        assert!(
            !started.changes.starter_mods_installed,
            "try again next time (L-7)"
        );
        assert_eq!(started.changes.loader_version.as_deref(), Some("0.19.5"));

        let lines = all_lines(&mut started.logs).await;
        assert!(lines.contains(&"arg: net.fabricmc.loader.impl.launch.knot.KnotClient".to_owned()));
        assert!(lines.contains(&"arg: -Dfabric.test=1".to_owned()));
        started.game.wait().await;

        // The picked loader is saved with the download mark.
        started.changes.apply(&mut instance);
        assert_eq!(instance.loader_version, "0.19.5");
        assert!(
            instance.is_fully_downloaded,
            "the key includes the picked loader"
        );
        assert!(instance.install_starter_fabric_mods);
    }

    /// The game can print its token (1.7.x logs its session id): it's
    /// hidden before a line reaches the logs.
    #[tokio::test]
    async fn the_access_token_never_reaches_the_logs() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, mut instance) = temp_instance(tmp.path());
        instance.version = "test-1".into();
        let launcher = launcher(&game, &data).await;
        let config = Config {
            java_path: Some(fake_java(tmp.path())),
            ..Config::default()
        };
        let player = Identity::online(
            "Steve",
            "069a79f444e94726a5befca90e38aaf5",
            Secret::new("secret-token-123"),
            None,
        );
        let mut started = launcher
            .launch(&instance, &config, &player, |_| {})
            .await
            .unwrap();
        let lines = all_lines(&mut started.logs).await;
        started.game.wait().await;
        assert!(lines.iter().any(|l| l.contains("<redacted>")), "{lines:?}");
        assert!(
            lines.iter().all(|l| !l.contains("secret-token-123")),
            "{lines:?}"
        );
    }

    /// Offline, Modrinth is out of reach too: rather than wait out every
    /// starter mod's timeouts on each launch, they wait for an online one.
    #[tokio::test]
    async fn offline_launches_leave_the_starter_mods_for_later() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, mut instance) = temp_instance(tmp.path());
        instance.loader = Loader::Fabric;
        instance.loader_version = "0.19.5".into();
        let profile = json!({
            "id": "fabric-loader-0.19.5-test-1", "inheritsFrom": "test-1", "type": "release",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
        });
        Mock::given(method("GET"))
            .and(path(
                "/fabric/v2/versions/loader/test-1/0.19.5/profile/json",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(profile))
            .mount(&game.server)
            .await;
        let config = Config {
            java_path: Some(fake_java(tmp.path())),
            ..Config::default()
        };
        // Online once, which caches the metadata.
        let online = launcher(&game, &data).await;
        let first = online
            .launch(&instance, &config, &Identity::offline("Alex"), |_| {})
            .await
            .unwrap();
        first.game.wait().await;

        // Then offline, with the starter mods still to install.
        instance.install_starter_fabric_mods = true;
        let dead = wiremock::MockServer::start().await;
        let mut offline = online.clone();
        let (http, uri) = (offline.http.clone(), dead.uri());
        offline.mojang =
            Mojang::with_manifest_url(http.clone(), &data, format!("{uri}/manifest.json"));
        offline.fabric =
            fabric::Fabric::with_meta_url(http.clone(), &data, format!("{uri}/fabric"));
        offline.modrinth = modrinth::Client::with_base_url(http, format!("{uri}/modrinth"));
        let steps = Steps::default();
        let started = offline
            .launch(&instance, &config, &Identity::offline("Alex"), |s| {
                steps.push(s);
            })
            .await
            .unwrap();
        assert!(started.stale);
        assert!(started.starter_mods.is_none());
        assert!(!steps.stages().contains(&Stage::StarterMods));
        assert!(!started.changes.starter_mods_installed, "still to do");
        let asked = dead.received_requests().await.unwrap_or_default();
        assert!(
            asked.iter().all(|r| !r.url.path().starts_with("/modrinth")),
            "{asked:?}"
        );
        started.game.wait().await;
    }

    #[tokio::test]
    async fn unsupported_loaders_and_missing_java_fail_cleanly() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, mut instance) = temp_instance(tmp.path());
        let launcher = launcher(&game, &data).await;

        instance.loader = Loader::Forge;
        let err = launcher
            .launch(
                &instance,
                &Config::default(),
                &Identity::offline(""),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "mctui can't launch Forge instances yet");

        instance.loader = Loader::Vanilla;
        let config = Config {
            java_path: Some(tmp.path().join("no-such-java")),
            ..Config::default()
        };
        let err = launcher
            .launch(&instance, &config, &Identity::offline(""), |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Java(_)), "{err}");
        let message = err.to_string();
        assert!(
            message.starts_with("the Java set in Settings (") && message.ends_with("doesn't exist"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn a_stale_instance_java_is_set_aside_and_forgotten() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, mut instance) = temp_instance(tmp.path());
        let mut launcher = launcher(&game, &data).await;
        // The Go app pinned the Java it picked; it's gone since.
        let gone = tmp.path().join("Cellar/openjdk/21.0.1/bin/java");
        instance.java_path = Some(gone.clone());
        let java = fake_java(&tmp.path().join("jvm"));
        launcher.runtimes.roots = vec![tmp.path().join("jvm")];

        let mut started = launcher
            .launch(
                &instance,
                &Config::default(),
                &Identity::offline(""),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(
            (&started.java.java, started.java.source),
            (&java, Source::System)
        );
        let stale = started.changes.stale_java.clone().unwrap();
        assert_eq!(
            stale,
            StaleJava {
                path: gone,
                why: Staleness::Missing
            }
        );
        assert!(stale.to_string().contains("no longer exists"), "{stale}");
        all_lines(&mut started.logs).await;
        started.game.wait().await;

        // The caller saves the instance without it.
        started.changes.apply(&mut instance);
        assert_eq!(instance.java_path, None);
        let again = launcher
            .launch(
                &instance,
                &Config::default(),
                &Identity::offline(""),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(again.changes.stale_java, None);
        again.game.wait().await;
    }

    #[tokio::test]
    async fn apple_silicon_runs_old_natives_on_an_intel_java() {
        let game = Game::start("test").await;
        let tmp = tempfile::tempdir().unwrap();
        let (data, instance) = temp_instance(tmp.path());
        let mut launcher = launcher(&game, &data).await;
        (launcher.runtimes.os, launcher.runtimes.arch) = MAC;
        launcher.runtimes.roots = vec![tmp.path().join("jvm")];
        let arm = fake_jdk(&tmp.path().join("jvm/arm"), "aarch64");
        let intel = fake_jdk(&tmp.path().join("jvm/intel"), "x86_64");

        // The test version's natives are Intel-only on macOS.
        let mut started = launcher
            .launch(
                &instance,
                &Config::default(),
                &Identity::offline(""),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(
            (&started.java.java, started.java.arch),
            (&intel, Arch::X86_64)
        );
        let lines = all_lines(&mut started.logs).await;
        assert!(lines.contains(&"arg: net.minecraft.client.main.Main".to_owned()));
        started.game.wait().await;

        // Chosen by hand, the arm64 one is refused before it can crash.
        let config = Config {
            java_path: Some(arm),
            ..Config::default()
        };
        let err = launcher
            .launch(&instance, &config, &Identity::offline(""), |_| {})
            .await
            .unwrap_err();
        let message = err.to_string();
        assert!(
            message.starts_with("the Java set in Settings (")
                && message.contains("is an arm64 Java"),
            "{message}"
        );
    }

    #[test]
    fn launching_can_run_on_a_spawned_task() {
        fn assert_send<T: Send>(_: &T) {}
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path());
        let runtimes = Runtimes {
            managed: tmp.path().to_owned(),
            go_managed: None,
            java_home: None,
            path: Vec::new(),
            roots: Vec::new(),
            adoptium_url: String::new(),
            os: Os::current(),
            arch: Arch::current(),
        };
        let launcher = Launcher::new(Http::new().unwrap(), data, runtimes);
        let (instance, config, identity) = (
            Instance::default(),
            Config::default(),
            Identity::offline(""),
        );
        let launch = launcher.launch(&instance, &config, &identity, |_| {});
        assert_send(&launch);
    }
}

/// Plans and prepares a real version into a temp dir with Mojang's real
/// services, then starts it if a Java that fits is installed, and stops it
/// once it has logged a few lines. `MCTUI_LIVE_VERSION` picks the version
/// (default `rd-132211`, the smallest).
#[tokio::test]
#[ignore = "downloads a real game from Mojang"]
async fn live_launch() {
    use std::time::{Duration, Instant};

    use mctui_core::java::Runtimes;
    use mctui_core::launch::Game;
    use mctui_core::mojang::Mojang;
    use mctui_core::net::Http;

    let id = std::env::var("MCTUI_LIVE_VERSION").unwrap_or_else(|_| "rd-132211".into());
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("data"));
    let instance = Instance {
        id: "Live".into(),
        name: "Live".into(),
        version: id.clone(),
        path: data.instances_dir().join("Live"),
        ..Instance::default()
    };
    std::fs::create_dir_all(&instance.path).unwrap();
    let http = Http::new().unwrap();

    let version = Mojang::new(http.clone(), &data)
        .resolve(&id)
        .await
        .unwrap()
        .value;
    let runtimes = Runtimes::system(&data).unwrap();
    let needs = runtimes.needs(&version);
    let required = needs.major;
    let java = runtimes
        .find(&needs, mctui_core::java::Choice::default())
        .await
        .unwrap()
        .runtime;
    let java_path = java
        .as_ref()
        .map_or_else(|| PathBuf::from("java"), |j| j.java.clone());
    let arch = java.as_ref().map_or(runtimes.arch, |j| j.arch);
    let plan = launch::plan(&PlanInputs {
        version: &version,
        instance: &instance,
        config: &Config::default(),
        data: &data,
        java: &java_path,
        identity: &Identity::offline("mctui"),
        env: &Env::new(runtimes.os, arch),
    })
    .unwrap();

    let started = Instant::now();
    let last = Mutex::new(None);
    let prepared = launch::prepare(&http, &plan, |step| *last.lock().unwrap() = Some(step))
        .await
        .unwrap();
    println!(
        "prepared {id} in {:.1?}: {} files, {} natives jars, verified: {}, last step {:?}",
        started.elapsed(),
        plan.files.len(),
        plan.natives.len(),
        prepared.verified,
        last.lock().unwrap()
    );
    println!("natives: {:?}", prepare::files_under(&plan.natives_dir));
    assert!(plan.files.iter().all(|f| f.dest.is_file()));

    let Some(java) = java else {
        println!("no installed Java {required} fits {id}; not starting it");
        return;
    };
    println!(
        "starting {id} with Java {} at {}",
        java.major,
        java.java.display()
    );
    let (game, mut logs) = Game::spawn(plan.command(), Duration::from_secs(10)).unwrap();
    let mut seen = 0;
    let first_lines = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(line) = logs.recv().await {
            println!("  [{:?}] {}", line.level, line.text);
            seen += 1;
            if seen == 5 {
                break;
            }
        }
    })
    .await;
    println!("saw {seen} lines (timed out: {})", first_lines.is_err());
    game.stop();
    let report = game.wait().await;
    println!("{report:?}");
    assert!(report.stopped.is_some() || seen > 0, "{report:?}");
}
