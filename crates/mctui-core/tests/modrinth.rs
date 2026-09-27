//! The Modrinth client and installer, against real API responses (trimmed,
//! in `tests/fixtures/modrinth`) and a mock server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use mctui_core::instance::{Instance, InstanceStore, Loader, NewInstance};
use mctui_core::modrinth::{
    self, Client, DependencyType, Error, Project, SearchResults, Side, SkipReason,
    UnresolvedReason, Version, choose_version,
};
use mctui_core::net::{Http, Progress};
use mctui_core::paths::DataDir;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture<T: DeserializeOwned>(name: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/modrinth")
        .join(name);
    serde_json::from_slice(&std::fs::read(&path).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn real_responses_decode() {
    let search: SearchResults = fixture("search-sodium.json");
    assert_eq!(search.total_hits, 34);
    let sodium = &search.hits[0];
    assert_eq!(
        (
            sodium.project_id.as_str(),
            sodium.slug.as_str(),
            sodium.title.as_str()
        ),
        ("AANobbMI", "sodium", "Sodium")
    );
    assert_eq!(sodium.author, "jellysquid3");
    assert_eq!(
        (sodium.client_side, sodium.server_side),
        (Side::Required, Side::Unsupported)
    );
    assert!(sodium.categories.iter().any(|c| c == "optimization"));
    assert!(sodium.icon_url.is_some());
    assert_eq!(
        sodium.date_modified.to_string(),
        "2026-09-20T21:27:06.329936Z"
    );

    let project: Project = fixture("project-sodium.json");
    assert_eq!(
        (project.id.as_str(), project.slug.as_str()),
        ("AANobbMI", "sodium")
    );
    let projects: Vec<Project> = fixture("projects.json");
    let slugs: Vec<_> = projects.iter().map(|p| p.slug.as_str()).collect();
    assert_eq!(slugs, ["fabric-api", "modmenu"]);

    let modmenu: Vec<Version> = fixture("versions-modmenu.json");
    let deps: Vec<_> = modmenu[0]
        .dependencies
        .iter()
        .map(|d| (d.project_id.as_deref(), d.dependency_type))
        .collect();
    assert_eq!(
        deps,
        [
            (Some("eXts2L7r"), DependencyType::Required),
            (Some("P7dR8mSH"), DependencyType::Required),
        ]
    );
}

#[test]
fn picks_versions_from_real_responses() {
    for (fixture_name, want_id, want_file) in [
        (
            "versions-sodium.json",
            "c3YkZvne",
            "sodium-fabric-0.6.13+mc1.21.4.jar",
        ),
        (
            "versions-fabric-api.json",
            "ZNwYCTsk",
            "fabric-api-0.118.0+1.21.4.jar",
        ),
        ("versions-modmenu.json", "qEKKsTqd", "modmenu-13.0.4.jar"),
    ] {
        let mut versions: Vec<Version> = fixture(fixture_name);
        for _ in 0..2 {
            let chosen = choose_version(&versions, "1.21.4", "fabric").unwrap();
            let jar = chosen.jar().unwrap();
            assert_eq!(
                (chosen.id.as_str(), jar.filename.as_str()),
                (want_id, want_file),
                "{fixture_name}"
            );
            assert_eq!(jar.hashes.sha1.len(), 40);
            // The date decides, not the order the API listed them in.
            versions.reverse();
        }
        assert_eq!(choose_version(&versions, "1.21.5", "fabric"), None);
        assert_eq!(choose_version(&versions, "1.21.4", "forge"), None);
    }
}

// --- A mock Modrinth ---------------------------------------------------------

struct Modrinth {
    server: MockServer,
    client: Client,
}

impl Modrinth {
    async fn start() -> Self {
        let server = MockServer::start().await;
        let client = Client::with_base_url(Http::new().unwrap(), server.uri());
        Self { server, client }
    }

    /// Serves `versions` as the Fabric 1.21.4 versions of `project` (an id or
    /// slug), and their jars, whose contents are `jar:<file name>`.
    async fn versions(&self, project: &str, mut versions: Vec<Value>) {
        for file in versions
            .iter_mut()
            .flat_map(|v| v["files"].as_array_mut().unwrap())
        {
            let name = file["filename"].as_str().unwrap().to_owned();
            let body = jar_body(&name);
            file["url"] = json!(format!("{}/files/{name}", self.server.uri()));
            file["hashes"]["sha1"] = json!(sha1_hex(&body));
            file["size"] = json!(body.len());
            self.get(
                &format!("/files/{name}"),
                ResponseTemplate::new(200).set_body_bytes(body),
            )
            .await;
        }
        Mock::given(method("GET"))
            .and(path(format!("/project/{project}/version")))
            .and(query_param("loaders", r#"["fabric"]"#))
            .and(query_param("game_versions", r#"["1.21.4"]"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(versions))
            .mount(&self.server)
            .await;
    }

    /// Serves `projects` from the bulk projects endpoint, whatever is asked.
    async fn projects(&self, projects: &[(&str, &str, &str)]) {
        let projects: Vec<Value> = projects
            .iter()
            .map(|&(id, slug, title)| {
                json!({"id": id, "slug": slug, "title": title, "updated": "2025-01-01T00:00:00Z"})
            })
            .collect();
        self.get(
            "/projects",
            ResponseTemplate::new(200).set_body_json(projects),
        )
        .await;
    }

    async fn get(&self, route: &str, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(response)
            .mount(&self.server)
            .await;
    }

    async fn install(
        &self,
        inst: &Instance,
        project: &str,
    ) -> modrinth::Result<modrinth::InstallReport> {
        self.client.install(inst, project, |_| {}).await
    }
}

/// A Fabric 1.21.4 release of `project` with one jar, needing `deps` as
/// (dependency type, project id) pairs.
fn version(project: &str, id: &str, file: &str, day: u8, deps: &[(&str, &str)]) -> Value {
    let deps: Vec<Value> = deps
        .iter()
        .map(|&(kind, dep)| json!({"project_id": dep, "version_id": null, "dependency_type": kind}))
        .collect();
    json!({
        "id": id,
        "project_id": project,
        "name": id,
        "version_number": id,
        "version_type": "release",
        "game_versions": ["1.21.4"],
        "loaders": ["fabric"],
        "date_published": format!("2025-01-{day:02}T00:00:00Z"),
        "dependencies": deps,
        "files": [{"url": "", "filename": file, "primary": true, "size": 0, "hashes": {"sha1": ""}}],
    })
}

fn jar_body(name: &str) -> Vec<u8> {
    format!("jar:{name}").into_bytes()
}

fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest as _;
    use std::fmt::Write as _;
    sha1::Sha1::digest(data)
        .iter()
        .fold(String::new(), |mut hex, b| {
            write!(hex, "{b:02x}").unwrap();
            hex
        })
}

fn fabric_instance() -> (tempfile::TempDir, Instance) {
    let dir = tempfile::tempdir().unwrap();
    let inst = InstanceStore::new(&DataDir::new(dir.path()))
        .create(NewInstance {
            name: "Fabric".to_owned(),
            version: "1.21.4".to_owned(),
            loader: Loader::Fabric,
            ..NewInstance::default()
        })
        .unwrap();
    (dir, inst)
}

fn mods_dir(inst: &Instance) -> PathBuf {
    inst.game_dir().join("mods")
}

/// The files in the mods folder but the catalog (and any temp file), sorted.
fn jars(inst: &Instance) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(mods_dir(inst))
        .into_iter()
        .flatten()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// The catalog's entries as (projectId, slug, file, title).
fn catalog(inst: &Instance) -> Vec<(String, String, String, Option<String>)> {
    let text = std::fs::read_to_string(mods_dir(inst).join(".mctui-modrinth.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    file["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let s = |key: &str| e[key].as_str().map(str::to_owned);
            (
                s("projectId").unwrap(),
                s("slug").unwrap(),
                s("file").unwrap(),
                s("title"),
            )
        })
        .collect()
}

fn entry(
    id: &str,
    slug: &str,
    file: &str,
    title: &str,
) -> (String, String, String, Option<String>) {
    (id.into(), slug.into(), file.into(), Some(title.into()))
}

// --- Search ------------------------------------------------------------------

#[tokio::test]
async fn search_filters_to_fabric_mods_for_the_game_version() {
    let modrinth = Modrinth::start().await;
    let body: Value = fixture("search-sodium.json");
    let facets = r#"[["categories:fabric"],["versions:1.21.4"],["project_type:mod"]]"#;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("facets", facets))
        .and(query_param("query", "sodium"))
        .and(query_param("index", "relevance"))
        .and(query_param("offset", "20"))
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&modrinth.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("facets", facets))
        .and(query_param("index", "downloads"))
        .and(query_param_is_missing("query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&modrinth.server)
        .await;

    let found = modrinth
        .client
        .search("  sodium ", "1.21.4", "fabric", 20, 10)
        .await
        .unwrap();
    assert_eq!(found.hits[0].slug, "sodium");
    let browsed = modrinth
        .client
        .search("", "1.21.4", "fabric", 0, 20)
        .await
        .unwrap();
    assert_eq!(browsed.hits.len(), 3);
}

#[tokio::test]
async fn rate_limits_are_waited_out() {
    let modrinth = Modrinth::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&modrinth.server)
        .await;
    modrinth
        .get(
            "/project/sodium",
            ResponseTemplate::new(200).set_body_json(fixture::<Value>("project-sodium.json")),
        )
        .await;
    let project = modrinth.client.project("sodium").await.unwrap();
    assert_eq!(project.id, "AANobbMI");
}

// --- Install -----------------------------------------------------------------

/// `root-mod` (ROOT) needs `dep-lib` (DEP), optionally `extras` (OPT), and
/// can't run with `rival` (RIVAL).
async fn serve_root_and_dep(modrinth: &Modrinth, root_file: &str, day: u8) {
    let root_deps = [
        ("required", "DEP"),
        ("optional", "OPT"),
        ("incompatible", "RIVAL"),
    ];
    modrinth
        .versions(
            "root-mod",
            vec![
                version("ROOT", "root-old", "root-0.jar", 1, &[]),
                version("ROOT", "root-new", root_file, day, &root_deps),
            ],
        )
        .await;
    modrinth
        .versions(
            "DEP",
            vec![version(
                "DEP",
                "dep-1",
                "dep-1.jar",
                1,
                &[("required", "ROOT")],
            )],
        )
        .await;
    modrinth
        .projects(&[
            ("ROOT", "root-mod", "Root Mod"),
            ("DEP", "dep-lib", "Dep Lib"),
            ("OPT", "extras", "Extras"),
        ])
        .await;
}

#[tokio::test]
async fn installs_a_mod_with_its_required_dependencies() {
    let modrinth = Modrinth::start().await;
    serve_root_and_dep(&modrinth, "root-1.jar", 5).await;
    let (_dir, inst) = fabric_instance();

    let progress = Mutex::new(Vec::new());
    let report = modrinth
        .client
        .install(&inst, "root-mod", |p| progress.lock().unwrap().push(p))
        .await
        .unwrap();

    let installed: Vec<_> = report
        .installed
        .iter()
        .map(|m| {
            (
                m.project_id.as_str(),
                m.title.as_str(),
                m.file.as_str(),
                m.dependency,
            )
        })
        .collect();
    assert_eq!(
        installed,
        [
            ("ROOT", "Root Mod", "root-1.jar", false),
            ("DEP", "Dep Lib", "dep-1.jar", true),
        ]
    );
    let skipped: Vec<_> = report
        .skipped
        .iter()
        .map(|d| (d.name.as_str(), d.reason))
        .collect();
    assert_eq!(
        skipped,
        [
            ("Extras", SkipReason::Optional),
            ("RIVAL", SkipReason::Incompatible { installed: false }),
        ]
    );
    assert!(report.unresolved.is_empty() && report.replaced.is_empty());

    assert_eq!(jars(&inst), ["dep-1.jar", "root-1.jar"]);
    assert_eq!(
        std::fs::read(mods_dir(&inst).join("root-1.jar")).unwrap(),
        jar_body("root-1.jar")
    );
    // The real project id, not the slug it was asked for by.
    assert_eq!(
        catalog(&inst),
        [
            entry("ROOT", "root-mod", "root-1.jar", "Root Mod"),
            entry("DEP", "dep-lib", "dep-1.jar", "Dep Lib"),
        ]
    );
    let last = *progress.lock().unwrap().last().unwrap();
    let bytes = (jar_body("root-1.jar").len() + jar_body("dep-1.jar").len()) as u64;
    assert_eq!(
        last,
        Progress {
            files_done: 2,
            files_total: 2,
            bytes_done: bytes,
            bytes_total: bytes,
        }
    );

    let listed: Vec<_> = modrinth::installed(&inst)
        .await
        .into_iter()
        .map(|m| (m.title, m.project_id))
        .collect();
    assert_eq!(
        listed,
        [
            ("Dep Lib".to_owned(), Some("DEP".to_owned())),
            ("Root Mod".to_owned(), Some("ROOT".to_owned())),
        ]
    );
}

#[tokio::test]
async fn reinstalling_upgrades_and_keeps_dependencies() {
    let modrinth = Modrinth::start().await;
    serve_root_and_dep(&modrinth, "root-1.jar", 5).await;
    let (_dir, inst) = fabric_instance();
    modrinth.install(&inst, "root-mod").await.unwrap();

    modrinth.server.reset().await;
    serve_root_and_dep(&modrinth, "root-2.jar", 9).await;
    let report = modrinth.install(&inst, "root-mod").await.unwrap();

    assert_eq!(report.replaced, ["root-1.jar"]);
    let installed: Vec<_> = report.installed.iter().map(|m| m.file.as_str()).collect();
    assert_eq!(installed, ["root-2.jar"]);
    assert!(
        report
            .skipped
            .iter()
            .any(|d| d.name == "Dep Lib" && d.reason == SkipReason::AlreadyInstalled)
    );
    assert_eq!(jars(&inst), ["dep-1.jar", "root-2.jar"]);
    assert_eq!(
        catalog(&inst),
        [
            entry("DEP", "dep-lib", "dep-1.jar", "Dep Lib"),
            entry("ROOT", "root-mod", "root-2.jar", "Root Mod"),
        ]
    );
    let fetched_dep_jar = modrinth
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/files/dep-1.jar");
    assert!(
        !fetched_dep_jar,
        "an installed dependency isn't downloaded again"
    );
}

#[tokio::test]
async fn unresolved_required_dependencies_are_reported() {
    let modrinth = Modrinth::start().await;
    let mut root = version("ROOT", "root-1", "root-1.jar", 1, &[("required", "GONE")]);
    root["dependencies"]
        .as_array_mut()
        .unwrap()
        .push(json!({"file_name": "OptiFine.jar", "dependency_type": "required"}));
    modrinth.versions("ROOT", vec![root]).await;
    modrinth.projects(&[("ROOT", "root-mod", "Root Mod")]).await;
    let (_dir, inst) = fabric_instance();

    let report = modrinth.install(&inst, "ROOT").await.unwrap();

    let unresolved: Vec<_> = report
        .unresolved
        .iter()
        .map(|d| (d.name.as_str(), d.reason))
        .collect();
    assert_eq!(
        unresolved,
        [
            ("GONE", UnresolvedReason::NoCompatibleVersion),
            ("OptiFine.jar", UnresolvedReason::NotOnModrinth),
        ]
    );
    assert_eq!(jars(&inst), ["root-1.jar"]);
}

#[tokio::test]
async fn a_failed_dependency_download_installs_nothing() {
    let modrinth = Modrinth::start().await;
    let (_dir, inst) = fabric_instance();
    // The dependency's jar is listed, but fails slowly: long after the
    // requested mod's jar would have landed, were they fetched together.
    modrinth
        .get(
            "/missing/dep-1.jar",
            ResponseTemplate::new(404).set_delay(Duration::from_millis(300)),
        )
        .await;
    modrinth
        .versions(
            "ROOT",
            vec![version(
                "ROOT",
                "root-1",
                "root-1.jar",
                1,
                &[("required", "DEP")],
            )],
        )
        .await;
    let mut dep = version("DEP", "dep-1", "dep-1.jar", 1, &[]);
    dep["files"][0]["url"] = json!(format!("{}/missing/dep-1.jar", modrinth.server.uri()));
    dep["files"][0]["hashes"]["sha1"] = json!("0".repeat(40));
    modrinth
        .get(
            "/project/DEP/version",
            ResponseTemplate::new(200).set_body_json(vec![dep]),
        )
        .await;
    modrinth.projects(&[]).await;

    let err = modrinth.install(&inst, "ROOT").await.unwrap_err();

    assert!(matches!(err, Error::Net(_)), "{err}");
    assert!(
        jars(&inst).is_empty(),
        "the requested mod waits for its dependencies"
    );
    assert!(!mods_dir(&inst).join(".mctui-modrinth.json").exists());
}

#[tokio::test]
async fn jars_the_catalog_does_not_vouch_for_are_identified_by_hash() {
    let modrinth = Modrinth::start().await;
    let (_dir, inst) = fabric_instance();
    let mods = mods_dir(&inst);
    std::fs::create_dir_all(&mods).unwrap();
    // What the Go app's starter wrote: the slug as the project id.
    std::fs::write(
        mods.join(".mctui-modrinth.json"),
        r#"{"projects": [{"projectId": "fabric-api", "slug": "fabric-api", "file": "fabric-api-0.118.0+1.21.4.jar"}]}"#,
    )
    .unwrap();
    std::fs::write(mods.join("fabric-api-0.118.0+1.21.4.jar"), "fapi").unwrap();
    std::fs::write(mods.join("handmade.jar"), "handmade").unwrap();
    let fabric_api = version(
        "P7dR8mSH",
        "fapi-1",
        "fabric-api-0.118.0+1.21.4.jar",
        1,
        &[],
    );
    modrinth
        .get(
            &format!("/version_file/{}", sha1_hex(b"fapi")),
            ResponseTemplate::new(200).set_body_json(fabric_api),
        )
        .await;
    // handmade.jar isn't on Modrinth: the mock answers 404.
    modrinth
        .versions(
            "modmenu",
            vec![version(
                "mOgUt4GM",
                "mm-1",
                "modmenu-13.0.4.jar",
                1,
                &[("required", "P7dR8mSH")],
            )],
        )
        .await;
    modrinth
        .projects(&[
            ("P7dR8mSH", "fabric-api", "Fabric API"),
            ("mOgUt4GM", "modmenu", "Mod Menu"),
        ])
        .await;

    let report = modrinth.install(&inst, "modmenu").await.unwrap();

    let skipped: Vec<_> = report
        .skipped
        .iter()
        .map(|d| (d.name.as_str(), d.reason))
        .collect();
    assert_eq!(skipped, [("Fabric API", SkipReason::AlreadyInstalled)]);
    assert_eq!(
        jars(&inst),
        [
            "fabric-api-0.118.0+1.21.4.jar",
            "handmade.jar",
            "modmenu-13.0.4.jar"
        ]
    );
    assert_eq!(
        catalog(&inst),
        [
            entry(
                "P7dR8mSH",
                "fabric-api",
                "fabric-api-0.118.0+1.21.4.jar",
                "Fabric API"
            ),
            entry("mOgUt4GM", "modmenu", "modmenu-13.0.4.jar", "Mod Menu"),
        ]
    );
}

#[tokio::test]
async fn vanilla_instances_take_no_mods() {
    let modrinth = Modrinth::start().await;
    let (_dir, mut inst) = fabric_instance();
    inst.loader = Loader::Vanilla;
    let err = modrinth.install(&inst, "sodium").await.unwrap_err();
    assert!(matches!(err, Error::NoModLoader), "{err}");
}

#[tokio::test]
async fn a_mod_with_no_fitting_version_is_an_error() {
    let modrinth = Modrinth::start().await;
    let (_dir, inst) = fabric_instance();
    modrinth.versions("lithium", vec![]).await;
    let err = modrinth.install(&inst, "lithium").await.unwrap_err();
    assert!(
        matches!(&err, Error::NoCompatibleVersion { project, .. } if project == "lithium"),
        "{err}"
    );
}

#[test]
fn installs_can_run_on_any_thread() {
    fn assert_send<T: Send>(_: &T) {}
    let client = Client::new(Http::new().unwrap());
    let inst = Instance::default();
    let install = client.install(&inst, "sodium", |_| {});
    assert_send(&install);
    let starter = client.install_starter(&inst, |_| {});
    assert_send(&starter);
}

// --- Installed mods ----------------------------------------------------------

#[tokio::test]
async fn installed_mods_are_named_and_removable() {
    let modrinth = Modrinth::start().await;
    serve_root_and_dep(&modrinth, "root-1.jar", 5).await;
    let (_dir, inst) = fabric_instance();
    modrinth.install(&inst, "root-mod").await.unwrap();
    std::fs::write(
        mods_dir(&inst).join("sodium-fabric-0.6.13+mc1.21.4.jar"),
        "x",
    )
    .unwrap();

    let listed = modrinth::installed(&inst).await;
    let rows: Vec<_> = listed
        .iter()
        .map(|m| (m.title.as_str(), m.version.as_deref(), m.file.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Dep Lib", Some("1"), "dep-1.jar"),
            ("Root Mod", Some("1"), "root-1.jar"),
            (
                "Sodium",
                Some("0.6.13"),
                "sodium-fabric-0.6.13+mc1.21.4.jar"
            ),
        ]
    );
    assert!(listed[1].is_project("ROOT", "root-mod"));
    assert!(listed[1].is_project("other-id", "root-mod"));
    assert!(!listed[2].is_project("AANobbMI", "sodium"));

    modrinth::remove(&inst, "root-1.jar").await.unwrap();
    assert_eq!(
        jars(&inst),
        ["dep-1.jar", "sodium-fabric-0.6.13+mc1.21.4.jar"]
    );
    assert_eq!(
        catalog(&inst),
        [entry("DEP", "dep-lib", "dep-1.jar", "Dep Lib")]
    );
    // Already gone is fine.
    modrinth::remove(&inst, "root-1.jar").await.unwrap();

    for bad in [
        "../instance.json",
        "sub/x.jar",
        "notes.txt",
        ".mctui-modrinth.json",
    ] {
        let err = modrinth::remove(&inst, bad).await.unwrap_err();
        assert!(matches!(err, Error::NotAJar(_)), "{bad}: {err}");
    }
}

// --- Starter mods --------------------------------------------------------------

/// The starter set as served from the real responses, but for `lithium`,
/// which has no version for 1.21.4 unless `with_lithium`.
async fn serve_starter(modrinth: &Modrinth, with_lithium: bool) {
    let jar_only = |fixture_name: &str| -> Vec<Value> {
        let versions: Vec<Value> = fixture(fixture_name);
        versions.into_iter().take(1).collect()
    };
    modrinth
        .versions("fabric-api", jar_only("versions-fabric-api.json"))
        .await;
    modrinth
        .versions("modmenu", jar_only("versions-modmenu.json"))
        .await;
    modrinth
        .versions("sodium", jar_only("versions-sodium.json"))
        .await;
    modrinth
        .versions(
            "eXts2L7r",
            vec![version("eXts2L7r", "pa-1", "placeholder-api.jar", 1, &[])],
        )
        .await;
    let lithium = if with_lithium {
        vec![version("gvQqBUqZ", "li-1", "lithium.jar", 1, &[])]
    } else {
        vec![]
    };
    modrinth.versions("lithium", lithium).await;
    modrinth
        .projects(&[
            ("P7dR8mSH", "fabric-api", "Fabric API"),
            ("mOgUt4GM", "modmenu", "Mod Menu"),
            ("AANobbMI", "sodium", "Sodium"),
            ("eXts2L7r", "placeholder-api", "Text Placeholder API"),
            ("gvQqBUqZ", "lithium", "Lithium"),
        ])
        .await;
}

#[tokio::test]
async fn starter_mods_install_what_they_can() {
    let modrinth = Modrinth::start().await;
    serve_starter(&modrinth, false).await;
    let (_dir, inst) = fabric_instance();

    let seen = Mutex::new(Vec::new());
    let report = modrinth
        .client
        .install_starter(&inst, |p| seen.lock().unwrap().push(p.index))
        .await;

    let outcomes: Vec<_> = report
        .mods
        .iter()
        .map(|m| {
            (
                m.starter.title,
                m.result
                    .as_ref()
                    .map(|r| r.installed.len())
                    .map_err(ToString::to_string),
            )
        })
        .collect();
    assert_eq!(
        outcomes,
        [
            ("Fabric API", Ok(1)),
            ("Mod Menu", Ok(2)),
            ("Sodium", Ok(1)),
            (
                "Lithium",
                Err("lithium has no version for Minecraft 1.21.4 with fabric".to_owned())
            ),
        ]
    );
    assert!(!report.all_ok());
    let mut indices = seen.into_inner().unwrap();
    indices.dedup();
    assert_eq!(indices, [0, 1, 2, 3]);
    // Mod Menu found the Fabric API installed a moment before by its real id.
    let ids: Vec<_> = catalog(&inst).into_iter().map(|e| e.0).collect();
    assert_eq!(ids, ["P7dR8mSH", "mOgUt4GM", "eXts2L7r", "AANobbMI"]);

    // Once Lithium catches up, trying again completes the set.
    modrinth.server.reset().await;
    serve_starter(&modrinth, true).await;
    let report = modrinth.client.install_starter(&inst, |_| {}).await;
    assert!(report.all_ok(), "{report:?}");
    assert_eq!(
        jars(&inst),
        [
            "fabric-api-0.118.0+1.21.4.jar",
            "lithium.jar",
            "modmenu-13.0.4.jar",
            "placeholder-api.jar",
            "sodium-fabric-0.6.13+mc1.21.4.jar",
        ]
    );
}

// --- Live --------------------------------------------------------------------

/// Talks to the real Modrinth: `cargo test -p mctui-core --test modrinth -- --ignored`.
#[tokio::test]
#[ignore = "needs the network"]
async fn live_search_and_version_pick() {
    let client = Client::new(Http::new().unwrap());
    let found = client
        .search("sodium", "1.21.4", "fabric", 0, 5)
        .await
        .unwrap();
    let sodium = found.hits.iter().find(|h| h.slug == "sodium").unwrap();
    let versions = client
        .versions(&sodium.project_id, &["fabric"], &["1.21.4"])
        .await
        .unwrap();
    let chosen = choose_version(&versions, "1.21.4", "fabric").unwrap();
    let jar = chosen.jar().unwrap();
    println!("{} → {} ({} bytes)", sodium.title, jar.filename, jar.size);
    assert!(
        jar.filename.starts_with("sodium-fabric-"),
        "{}",
        jar.filename
    );
    assert!(chosen.game_versions.iter().any(|v| v == "1.21.4"));
}
