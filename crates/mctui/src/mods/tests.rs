//! Mods screen tests: the whole app, against a mock Modrinth serving real
//! responses (from `mctui-core/tests/fixtures/modrinth`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEventKind};
use mctui_core::modrinth;
use mctui_core::net::{Http, Progress};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

use super::{DEBOUNCE, Mods, Msg, Pane, Target};
use crate::app::tests::Harness;
use crate::data::Data;
use crate::screen::Screen as _;
use crate::task::Spawner;
use crate::{testing, ui};

// --- A mock Modrinth ----------------------------------------------------------

/// Modrinth's answers, on an [`Harness::online`] harness's mock server.
trait Modrinth {
    /// Serves `body` for the most downloaded mods (`query: None`) or a
    /// search, at `offset`.
    fn serve_search(&self, query: Option<&str>, offset: usize, body: &Value);
    /// Serves `versions` as `project`'s Fabric 1.21.4 versions, and their
    /// jars (`size` bytes each, arriving after `delay`).
    fn serve_versions(&self, project: &str, versions: Vec<Value>, size: usize, delay: Duration);
    /// Serves every project the installs here ask about.
    fn serve_projects(&self);

    /// The real "sodium" search as the most downloaded mods.
    fn serve_popular(&self) {
        self.serve_search(None, 0, &fixture("search-sodium.json"));
    }

    /// Mod Menu, which needs Text Placeholder API and the Fabric API. The
    /// Fabric API's jar takes `delay` to arrive.
    fn serve_mod_menu(&self, delay: Duration) {
        let mod_menu = list(fixture("versions-modmenu.json"));
        self.serve_versions("mOgUt4GM", mod_menu, 1_050_000, ZERO);
        let placeholder = version("eXts2L7r", "placeholder-api-2.5.2+1.21.4.jar", &[]);
        self.serve_versions("eXts2L7r", vec![placeholder], 310_000, ZERO);
        let fabric_api = list(fixture("versions-fabric-api.json"));
        self.serve_versions("P7dR8mSH", fabric_api[..1].to_vec(), 2_140_000, delay);
        self.serve_projects();
    }

    /// Continuity, which needs Indium, which has nothing for 1.21.4.
    fn serve_continuity(&self) {
        let continuity = version("1IjD5062", "continuity-3.0.0+1.21.4.jar", &["Orvt0mRa"]);
        self.serve_versions("1IjD5062", vec![continuity], 180_000, ZERO);
        self.serve_versions("Orvt0mRa", vec![], 0, ZERO);
        self.serve_projects();
    }
}

impl Modrinth for Harness {
    fn serve_search(&self, query: Option<&str>, offset: usize, body: &Value) {
        let mock = Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("offset", offset.to_string()));
        let mock = match query {
            None => mock
                .and(query_param("index", "downloads"))
                .and(query_param_is_missing("query")),
            Some(q) => mock.and(query_param("query", q)),
        };
        self.mock(mock.respond_with(ResponseTemplate::new(200).set_body_json(body)));
    }

    fn serve_versions(
        &self,
        project: &str,
        mut versions: Vec<Value>,
        size: usize,
        delay: Duration,
    ) {
        for file in versions
            .iter_mut()
            .flat_map(|v| v["files"].as_array_mut().unwrap())
        {
            let name = file["filename"].as_str().unwrap().to_owned();
            let body = jar(&name, size);
            file["url"] = json!(format!("{}/files/{name}", self.uri()));
            file["hashes"]["sha1"] = json!(sha1_hex(&body));
            file["size"] = json!(body.len());
            let jar = ResponseTemplate::new(200)
                .set_body_bytes(body)
                .set_delay(delay);
            self.mock(
                Mock::given(method("GET"))
                    .and(path(format!("/files/{name}")))
                    .respond_with(jar),
            );
        }
        self.mock(
            Mock::given(method("GET"))
                .and(path(format!("/project/{project}/version")))
                .respond_with(ResponseTemplate::new(200).set_body_json(versions)),
        );
    }

    fn serve_projects(&self) {
        let mut projects = list(fixture("projects.json"));
        for (id, slug, title) in [
            ("eXts2L7r", "placeholder-api", "Text Placeholder API"),
            ("1IjD5062", "continuity", "Continuity"),
            ("Orvt0mRa", "indium", "Indium"),
        ] {
            projects.push(json!({
                "id": id, "slug": slug, "title": title, "updated": "2025-01-01T00:00:00Z"
            }));
        }
        self.mock(
            Mock::given(method("GET"))
                .and(path("/projects"))
                .respond_with(ResponseTemplate::new(200).set_body_json(projects)),
        );
    }
}

/// Modrinth answering 503 to every search.
fn serve_outage(h: &Harness) {
    let busy = ResponseTemplate::new(503).insert_header("retry-after", "0");
    h.mock(
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(busy),
    );
}

const ZERO: Duration = Duration::ZERO;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../mctui-core/tests/fixtures/modrinth")
        .join(name);
    serde_json::from_slice(&std::fs::read(&path).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn list(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        other => panic!("not a list: {other}"),
    }
}

/// A search result shaped like Modrinth's, from the Sodium one.
fn hit(
    id: &str,
    slug: &str,
    title: &str,
    author: &str,
    downloads: u64,
    description: &str,
) -> Value {
    let mut hit = fixture("search-sodium.json")["hits"][0].clone();
    hit["project_id"] = json!(id);
    hit["slug"] = json!(slug);
    hit["title"] = json!(title);
    hit["author"] = json!(author);
    hit["downloads"] = json!(downloads);
    hit["description"] = json!(description);
    hit
}

/// A Fabric 1.21.4 release of `project` with one jar, requiring `needs`.
fn version(project: &str, file: &str, needs: &[&str]) -> Value {
    let needs: Vec<Value> = needs
        .iter()
        .map(|id| json!({"project_id": id, "version_id": null, "dependency_type": "required"}))
        .collect();
    json!({
        "id": format!("{project}-1"), "project_id": project, "version_number": "1.0.0",
        "version_type": "release", "game_versions": ["1.21.4"], "loaders": ["fabric"],
        "date_published": "2025-01-01T00:00:00Z", "dependencies": needs,
        "files": [{"url": "", "filename": file, "primary": true, "size": 0, "hashes": {"sha1": ""}}]
    })
}

fn continuity_hit() -> Value {
    hit(
        "1IjD5062",
        "continuity",
        "Continuity",
        "PepperCode1",
        12_804_331,
        "A Fabric mod that allows for efficient connected textures.",
    )
}

fn results(hits: &[Value], total: usize) -> Value {
    json!({"hits": hits, "total_hits": total})
}

fn mod_menu_hit() -> Value {
    let mut hit = hit(
        "mOgUt4GM",
        "modmenu",
        "Mod Menu",
        "Prospector",
        104_523_441,
        "Adds a mod menu to view the list of mods you have installed.",
    );
    hit["display_categories"] = json!(["fabric", "library", "utility"]);
    hit["client_side"] = json!("required");
    hit["server_side"] = json!("unsupported");
    hit
}

fn jar(name: &str, size: usize) -> Vec<u8> {
    name.bytes().cycle().take(size.max(name.len())).collect()
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

// --- The app ------------------------------------------------------------------

/// Records that mctui installed the fixture's Sodium jar, so it has a
/// project (and shows as installed in results).
fn with_catalog(data: &mut Data) {
    let inst = &data.instances[0];
    let catalog = json!({"projects": [{
        "projectId": "AANobbMI", "slug": "sodium", "title": "Sodium",
        "file": "sodium-fabric-0.6.0+mc1.21.4.jar"
    }]});
    std::fs::write(
        mods_dir(inst).join(".mctui-modrinth.json"),
        catalog.to_string(),
    )
    .unwrap();
}

fn mods_dir(inst: &mctui_core::instance::Instance) -> PathBuf {
    inst.game_dir().join("mods")
}

/// Opens the mods screen for the selected instance.
fn open(h: &mut Harness) {
    h.press(KeyCode::Char('m'));
    settle(h);
}

/// Handles background work until it goes quiet (and the screen has had
/// time to finish anything that pauses between messages).
fn settle(h: &mut Harness) {
    h.settle();
    h.settle();
}

/// Lets the typed query's pause run out.
fn debounce(h: &mut Harness) {
    h.app.tick(Instant::now() + DEBOUNCE);
    settle(h);
}

fn row(list: &'static str, key: &str) -> ui::Target {
    ui::Target::Row {
        list,
        key: key.into(),
    }
}

fn jars(inst: &mctui_core::instance::Instance) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(mods_dir(inst))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| {
            std::path::Path::new(n)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        })
        .collect();
    names.sort();
    names
}

// --- Browsing -----------------------------------------------------------------

#[test]
fn opens_on_popular_mods_with_details() {
    let mut h = Harness::online(100, 30, with_catalog);
    h.serve_popular();
    open(&mut h);
    insta::assert_snapshot!(h.text());
}

#[test]
fn search_results_get_a_details_column_when_wide() {
    let mut h = Harness::online(130, 32, with_catalog);
    h.serve_popular();
    let found = results(&[mod_menu_hit()], 1);
    h.serve_search(Some("mod menu"), 0, &found);
    open(&mut h);
    h.press(KeyCode::Char('/'));
    h.type_text("mod menu");
    h.press(KeyCode::Enter);
    settle(&mut h);
    insta::assert_snapshot!(h.text());
}

#[test]
fn typing_searches_once_it_pauses() {
    let mut h = Harness::online(100, 30, |_| {});
    h.serve_popular();
    h.serve_search(Some("mod"), 0, &results(&[mod_menu_hit()], 1));
    open(&mut h);
    assert_eq!(h.requests("/search"), 1, "popular mods, on opening");

    h.press(KeyCode::Char('/'));
    h.type_text("mod");
    settle(&mut h);
    assert_eq!(h.requests("/search"), 1, "still typing");
    assert!(h.text().contains("Sodium Extra"), "the old results stay");

    debounce(&mut h);
    assert_eq!(h.requests("/search"), 2, "one search for the pause");
    let text = h.text();
    assert!(
        text.contains("Mod Menu") && !text.contains("Sodium Extra"),
        "{text}"
    );
    assert!(h.app.next_deadline().is_none(), "nothing left to wait for");
}

#[test]
fn up_from_the_first_result_reaches_the_field_and_q_types_there() {
    let mut h = Harness::online(100, 30, |_| {});
    h.serve_popular();
    open(&mut h);
    h.press(KeyCode::Up);
    h.type_text("q?:");
    assert!(!h.app.should_quit());
    assert!(h.text().contains("/ q?:"), "{}", h.text());
    h.press(KeyCode::Esc);
    h.press(KeyCode::Esc);
    assert!(h.text().contains("INSTANCES"), "a second esc goes home");
}

#[test]
fn more_results_load_near_the_end() {
    let mut h = Harness::online(100, 30, |_| {});
    h.serve_popular();
    let next = results(&[mod_menu_hit()], 34);
    h.serve_search(None, 20, &next);
    open(&mut h);
    assert!(h.text().contains("3 of 34"));
    h.press(KeyCode::Down);
    assert!(h.text().contains("Loading more…"), "{}", h.text());
    settle(&mut h);
    let text = h.text();
    assert!(
        text.contains("4 of 34") && text.contains("Mod Menu"),
        "{text}"
    );
}

#[test]
fn failed_searches_say_so_and_r_retries() {
    let mut h = Harness::online(100, 30, |_| {});
    h.mock(
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "0"))
            .up_to_n_times(3),
    );
    h.serve_popular();
    open(&mut h);
    insta::assert_snapshot!(h.text());
    h.press(KeyCode::Char('r'));
    settle(&mut h);
    assert!(h.text().contains("Sodium Extra"), "{}", h.text());
}

#[test]
fn empty_instance_and_no_matches() {
    let mut h = Harness::online(100, 30, |_| {});
    h.serve_popular();
    h.serve_search(Some("zzz"), 0, &results(&[], 0));
    h.press(KeyCode::Down); // Snapshot Test: Fabric, no mods.
    open(&mut h);
    h.press(KeyCode::Char('/'));
    h.type_text("zzz");
    h.press(KeyCode::Enter);
    settle(&mut h);
    insta::assert_snapshot!(h.text());
}

#[test]
fn the_fabric_gate_stays_on_home() {
    let mut h = Harness::new(100, 30);
    h.press(KeyCode::Char('G'));
    h.press(KeyCode::Up); // Creative Build: vanilla.
    h.press(KeyCode::Char('m'));
    assert_eq!(
        h.banner(),
        Some("The mods browser works with Fabric instances.")
    );
    assert!(h.text().contains("INSTANCES"), "still home");
}

// --- Installing -----------------------------------------------------------------

#[test]
fn installs_a_mod_with_its_dependencies() {
    let mut h = Harness::online(100, 30, with_catalog);
    h.serve_search(None, 0, &results(&[mod_menu_hit()], 1));
    h.serve_mod_menu(ZERO);
    open(&mut h);
    h.press(KeyCode::Enter);
    settle(&mut h);

    assert_eq!(h.banner(), Some("Installed Mod Menu (+2 dependencies)."));
    let inst = h.data().instances[0].clone();
    let jars = jars(&inst);
    for jar in [
        "modmenu-13.0.4.jar",
        "placeholder-api-2.5.2+1.21.4.jar",
        "fabric-api-0.118.0+1.21.4.jar",
    ] {
        assert!(jars.iter().any(|j| j == jar), "{jar} in {jars:?}");
    }
    let text = h.text();
    assert!(text.contains("INSTALLED   6"), "{text}");
    assert!(text.contains("✓ Installed"), "the result says so: {text}");
    assert!(text.contains("Update  ↵"), "{text}");

    // Again: it's up to date, and nothing downloads twice.
    h.press(KeyCode::Enter);
    settle(&mut h);
    assert_eq!(h.banner(), Some("Mod Menu is up to date."));
    assert_eq!(h.requests("/files/modmenu-13.0.4.jar"), 1);
}

#[test]
fn warns_when_a_dependency_is_missing() {
    let mut h = Harness::online(100, 30, |_| {});
    h.serve_search(None, 0, &results(&[continuity_hit()], 1));
    h.serve_continuity();
    open(&mut h);
    h.press(KeyCode::Enter);
    settle(&mut h);
    assert_eq!(h.banner(), Some("Installed Continuity."));
    let text = h.text();
    assert!(
        text.contains("▲  Couldn't find Indium for 1.21.4 — Continuity may"),
        "{text}"
    );
    // Searching again moves on.
    h.press(KeyCode::Char('/'));
    h.type_text("x");
    assert!(!h.text().contains("Indium"));
}

#[test]
fn shows_progress_and_esc_cancels_in_place() {
    let mut h = Harness::online(100, 30, with_catalog);
    h.serve_search(None, 0, &results(&[mod_menu_hit()], 1));
    h.serve_mod_menu(Duration::from_secs(30));
    open(&mut h);
    h.press(KeyCode::Enter);
    settle(&mut h);
    insta::assert_snapshot!(h.text());

    // A second install waits its turn.
    h.press(KeyCode::Enter);
    assert_eq!(
        h.banner(),
        Some("Mod Menu is still installing — one install at a time.")
    );

    h.press(KeyCode::Esc);
    assert_eq!(h.banner(), Some("Stopped installing Mod Menu."));
    let text = h.text();
    assert!(
        text.contains("Mods") && !text.contains("Installing"),
        "{text}"
    );
    let inst = h.data().instances[0].clone();
    assert!(
        !jars(&inst).iter().any(|j| j.starts_with("modmenu")),
        "the mod itself never lands"
    );
    h.press(KeyCode::Esc);
    assert!(h.text().contains("INSTANCES"), "then esc goes home");
}

#[test]
fn double_click_installs_and_the_button_does_too() {
    let mut h = Harness::online(100, 30, |_| {});
    h.serve_search(None, 0, &results(&[mod_menu_hit()], 1));
    h.serve_mod_menu(Duration::from_secs(30));
    open(&mut h);
    let at = h.find(&row("mods-results", "mOgUt4GM"));
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    assert!(h.text().contains("Installing Mod Menu"), "{}", h.text());
    h.press(KeyCode::Esc);

    h.click(&ui::Target::Mods(Target::Install));
    assert!(h.text().contains("Installing Mod Menu"), "{}", h.text());
}

// --- Installed ------------------------------------------------------------------

#[test]
fn installed_mods_are_listed_and_removed_after_confirming() {
    let mut h = Harness::online(100, 30, with_catalog);
    h.serve_popular();
    open(&mut h);
    h.press(KeyCode::Left);
    h.press(KeyCode::Char('G'));
    insta::assert_snapshot!(h.text());

    h.press(KeyCode::Char('d'));
    assert!(h.text().contains("Remove Sodium from “Survival 1.21”?"));
    h.press(KeyCode::Enter);
    let inst = h.data().instances[0].clone();
    assert_eq!(jars(&inst).len(), 3, "the dialog opens on Cancel");

    h.press(KeyCode::Char('d'));
    h.press(KeyCode::Left);
    h.press(KeyCode::Enter);
    settle(&mut h);
    assert_eq!(h.banner(), Some("Removed Sodium."));
    assert!(!jars(&inst).iter().any(|j| j.starts_with("sodium")));
    let text = h.text();
    assert!(
        text.contains("INSTALLED   2") && !text.contains("✓ Installed"),
        "{text}"
    );
}

#[test]
fn right_click_offers_the_row_actions() {
    let mut h = Harness::online(100, 30, with_catalog);
    h.serve_popular();
    open(&mut h);
    let at = h.find(&row("mods-results", "AANobbMI"));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    let text = h.text();
    assert!(
        text.contains("Update") && text.contains("Open Modrinth page") && text.contains("Remove"),
        "{text}"
    );
    h.press(KeyCode::Char('o'));
    assert_eq!(
        h.banner(),
        Some("Opened Sodium on Modrinth in your browser.")
    );

    // A mod mctui didn't install has no page to open.
    let at = h.find(&row("mods-installed", "lithium-fabric-0.14.3+mc1.21.4.jar"));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    let text = h.text();
    assert!(
        text.contains("Remove") && !text.contains("Open Modrinth page"),
        "{text}"
    );
}

// --- Layout ---------------------------------------------------------------------

#[test]
fn compact_shows_one_pane_at_a_time() {
    let mut h = Harness::online(70, 24, with_catalog);
    h.serve_popular();
    open(&mut h);
    insta::assert_snapshot!(h.text());
    h.press(KeyCode::Left);
    let text = h.text();
    assert!(
        text.contains("INSTALLED") && !text.contains("DISCOVER"),
        "{text}"
    );
    h.press(KeyCode::Tab);
    assert!(
        h.text().contains("DISCOVER"),
        "tab goes on to the search field"
    );
    h.press(KeyCode::Esc);
    h.click(&ui::Target::Mods(Target::Pane(Pane::Installed)));
    assert!(h.text().contains("INSTALLED"));
}

#[test]
fn never_panics_at_any_size() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let testing::Fixture { dir: _dir, data } = testing::fixture();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let tasks = Spawner::new(tx, runtime.handle().clone()).for_screen(1);
    let client = modrinth::Client::with_base_url(Http::new().unwrap(), "");
    let mut mods = Mods::new(&data.instances[0], tasks.clone(), client);
    let found = serde_json::from_value(fixture("search-sodium.json")).unwrap();
    mods.on_msg(Msg::Found {
        seq: 1,
        result: Ok(found),
    });
    let installed = modrinth::InstalledMod {
        file: "sodium.jar".into(),
        title: "Sodium".into(),
        version: Some("0.6.13".into()),
        size: 1_306_799,
        project_id: Some("AANobbMI".into()),
        slug: Some("sodium".into()),
    };
    mods.on_msg(Msg::Listed(vec![installed]));
    let theme = crate::theme::Theme::new(&crate::theme::DARK, None);
    let cx = ui::Cx {
        theme,
        hover: None,
        now: testing::now(),
        tz: testing::tz(),
    };

    let states: [&dyn Fn(&mut Mods); 4] = [
        &|_| {},
        &|m| {
            m.notes = vec!["Couldn't find Indium for 1.21.4 — Sodium may not load.".into()];
        },
        &|m| {
            m.install = Some(super::Install {
                seq: 9,
                project_id: "AANobbMI".into(),
                title: "Sodium".into(),
                update: true,
                progress: Some(Progress {
                    files_done: 1,
                    files_total: 3,
                    bytes_done: 1 << 20,
                    bytes_total: 3 << 20,
                }),
                started: Instant::now(),
                _task: tasks.spawn(|_| async {}),
            });
        },
        &|m| m.focus = super::Focus::Search,
    ];
    for state in states {
        state(&mut mods);
        for w in (0..=150).step_by(7) {
            for hgt in (0..=44).step_by(3) {
                let area = Rect::new(0, 0, w, hgt);
                mods.set_body(area);
                let mut buf = Buffer::empty(area);
                let mut cv = ui::Canvas::new(&mut buf, &cx);
                mods.render(&mut cv, &data);
                mods.on_key(KeyEvent::from(KeyCode::Down), Instant::now());
            }
        }
    }
}

/// A popular page with the mods players know, for screenshots.
fn popular_mods() -> Value {
    results(
        &[
            hit(
                "P7dR8mSH",
                "fabric-api",
                "Fabric API",
                "modmuss50",
                262_664_169,
                "Lightweight and modular API providing common hooks and intercompatibility measures utilized by mods using the Fabric toolchain.",
            ),
            fixture("search-sodium.json")["hits"][0].clone(),
            mod_menu_hit(),
            hit(
                "YL57xq9U",
                "iris",
                "Iris Shaders",
                "coderbot",
                98_421_337,
                "A modern shader pack loader for Minecraft intended to be compatible with existing OptiFine shader packs.",
            ),
            hit(
                "gvQqBUqZ",
                "lithium",
                "Lithium",
                "jellysquid3",
                76_144_902,
                "No-compromises game logic optimization mod. Well suited for clients and servers of all kinds.",
            ),
            hit(
                "9s6osm5g",
                "cloth-config",
                "Cloth Config API",
                "shedaniel",
                71_055_120,
                "Configuration Library for Minecraft Mods.",
            ),
            hit(
                "mOgUt4GX",
                "entityculling",
                "Entity Culling",
                "tr7zw",
                60_312_004,
                "Using async path-tracing to hide Block-/Entities that are not visible.",
            ),
            hit(
                "uXXizFIs",
                "ferrite-core",
                "FerriteCore",
                "malte0811",
                58_774_311,
                "Memory usage optimizations.",
            ),
        ],
        5056,
    )
}

/// Writes ANSI renders of the mods screen to `$MCTUI_SCREENSHOTS` for
/// `scripts/ansi2png.py`. Run with `--ignored`.
#[test]
#[ignore = "writes screenshots when MCTUI_SCREENSHOTS is set"]
fn screenshots() {
    let Some(dir) = std::env::var_os("MCTUI_SCREENSHOTS") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let save = |name: &str, h: &mut Harness| {
        std::fs::write(
            dir.join(format!("{name}.ansi")),
            crate::testing::ansi(&h.draw()),
        )
        .unwrap();
    };
    // The popular mods players know, Sodium's real search, and Mod Menu
    // installing slowly.
    let showcase = |w, hgt, edit: fn(&mut Data)| {
        let mut h = Harness::online(w, hgt, edit);
        h.serve_search(None, 0, &popular_mods());
        h.serve_search(Some("sodium"), 0, &fixture("search-sodium.json"));
        h.serve_mod_menu(Duration::from_secs(60));
        open(&mut h);
        h
    };

    let mut h = showcase(100, 30, with_catalog);
    save("mods", &mut h);
    h.press(KeyCode::Down);
    h.press(KeyCode::Down);
    let at = h.find(&row("mods-results", "YL57xq9U"));
    h.mouse(MouseEventKind::Moved, at);
    save("mods-hover", &mut h);
    h.press(KeyCode::Enter);
    settle(&mut h);
    save("mods-installing", &mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Left);
    save("mods-installed", &mut h);
    h.press(KeyCode::Char('d'));
    save("mods-remove", &mut h);
    h.press(KeyCode::Esc);
    let at = h.find(&row("mods-installed", "sodium-fabric-0.6.0+mc1.21.4.jar"));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    save("mods-menu", &mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    h.type_text("sodium");
    debounce(&mut h);
    save("mods-search", &mut h);

    let mut warned = Harness::online(100, 30, with_catalog);
    warned.serve_search(None, 0, &results(&[continuity_hit(), mod_menu_hit()], 2));
    warned.serve_continuity();
    open(&mut warned);
    warned.press(KeyCode::Enter);
    settle(&mut warned);
    save("mods-warnings", &mut warned);

    save("mods-80x24", &mut showcase(80, 24, with_catalog));
    save("mods-wide", &mut showcase(140, 36, with_catalog));
    save("mods-compact", &mut showcase(70, 24, with_catalog));
    let mut empty = Harness::online(100, 30, |_| {});
    empty.press(KeyCode::Down);
    empty.serve_search(None, 0, &popular_mods());
    open(&mut empty);
    save("mods-empty", &mut empty);

    let mut error = Harness::online(100, 30, with_catalog);
    serve_outage(&error);
    open(&mut error);
    save("mods-error", &mut error);

    let mut light = showcase(100, 30, |data| {
        with_catalog(data);
        data.config.theme = "light".into();
    });
    save("mods-light", &mut light);
}
