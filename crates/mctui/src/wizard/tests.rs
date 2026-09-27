//! The wizard end to end, against a mock Mojang and Fabric serving real
//! (trimmed) responses from `mctui-core/tests/fixtures`.

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use mctui_core::config::Config;
use mctui_core::instance::{Instance, Loader};
use ratatui::layout::Position;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{Step, Target};
use crate::app::tests::Harness;
use crate::ui;

fn fixture(name: &str) -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../mctui-core/tests/fixtures/");
    std::fs::read(format!("{path}{name}")).unwrap()
}

fn serve(h: &Harness, at: &str, response: ResponseTemplate) {
    h.mock(
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(response),
    );
}

/// Home with a stand-in for the online services that answers nothing yet.
fn unanswered(w: u16, hgt: u16) -> Harness {
    Harness::online(w, hgt, |_| {})
}

fn serve_versions(h: &Harness) {
    let body = fixture("mojang/version_manifest_v2.json");
    let at = "/mc/game/version_manifest_v2.json";
    serve(h, at, ResponseTemplate::new(200).set_body_bytes(body));
}

/// Fabric offers its loaders for `game` (the fixture's newest stable is
/// 0.19.5).
fn serve_fabric(h: &Harness, game: &str) {
    let body = fixture("fabric/loader_versions_1.21.8.json");
    let at = format!("/v2/versions/loader/{game}");
    serve(h, &at, ResponseTemplate::new(200).set_body_bytes(body));
}

/// Fabric's answer for a game version it doesn't know.
fn no_fabric(h: &Harness, game: &str) {
    let at = format!("/v2/versions/loader/{game}");
    serve(h, &at, ResponseTemplate::new(400));
}

/// Home with Mojang serving its versions and Fabric supporting 1.21.8.
fn serving(w: u16, hgt: u16) -> Harness {
    let h = unanswered(w, hgt);
    serve_versions(&h);
    serve_fabric(&h, "1.21.8");
    no_fabric(&h, "26.3");
    h
}

/// The wizard open with the versions loaded.
fn opened(w: u16, hgt: u16) -> Harness {
    let mut h = serving(w, hgt);
    h.press(KeyCode::Char('n'));
    h.settle();
    h
}

/// On the name step for 1.21.8 with `loader` (Fabric or Vanilla).
fn at_name_step(loader: &Loader) -> Harness {
    let mut h = opened(100, 30);
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    h.settle();
    if *loader == Loader::Vanilla {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
    h
}

fn created(h: &Harness, name: &str) -> Instance {
    let on_disk = h.data().store.scan().unwrap().instances;
    on_disk
        .into_iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("no instance named {name:?} on disk"))
}

/// A summary row as drawn: the label in its column, then the value.
fn summary(label: &str, value: &str) -> String {
    format!("{label:<14}{value}")
}

fn step(step: Step) -> ui::Target {
    ui::Target::Wizard(Target::Step(step))
}

fn version_row(id: &str) -> ui::Target {
    ui::Target::Row {
        list: super::versions::LIST,
        key: id.to_owned(),
    }
}

fn loader_row(key: &str) -> ui::Target {
    ui::Target::Row {
        list: super::loaders::LIST,
        key: key.to_owned(),
    }
}

#[test]
fn versions_load_newest_release_first() {
    let mut h = serving(100, 30);
    h.press(KeyCode::Char('n'));
    insta::assert_snapshot!("loading", h.text());
    h.settle();
    insta::assert_snapshot!("versions", h.text());
}

#[test]
fn offline_shows_the_saved_versions() {
    let mut h = unanswered(100, 30);
    let cache = h.data().dir.cache_dir();
    std::fs::create_dir_all(&cache).unwrap();
    let saved = fixture("mojang/version_manifest_v2.json");
    std::fs::write(cache.join("version_manifest_v2.json"), saved).unwrap();
    h.press(KeyCode::Char('n'));
    h.settle();
    insta::assert_snapshot!("offline", h.text());

    // r tries Mojang again, keeping the saved list up meanwhile.
    serve_versions(&h);
    h.press(KeyCode::Char('r'));
    assert!(h.text().contains("Reconnecting to Mojang"));
    h.settle();
    let text = h.text();
    assert!(
        !text.contains("Offline") && text.contains("1.21.8"),
        "{text}"
    );
}

#[test]
fn a_failed_load_offers_a_retry() {
    let mut h = unanswered(100, 30);
    h.press(KeyCode::Char('n'));
    h.settle();
    insta::assert_snapshot!("error", h.text());
    serve_versions(&h);
    h.click(&ui::Target::Key(KeyCode::Char('r')));
    h.settle();
    assert!(h.text().contains("1.21.8"), "loaded after the retry");
}

#[test]
fn space_shows_snapshots_and_remembers() {
    let mut h = opened(100, 30);
    assert!(!h.text().contains("26.4-snapshot-1"));
    h.press(KeyCode::Char(' '));
    let text = h.text();
    assert!(
        text.contains("26.4-snapshot-1")
            && text.contains("b1.7.3")
            && text.contains("ALL VERSIONS"),
        "{text}"
    );
    assert!(h.data().config.show_snapshots);
    assert!(Config::load(&h.data().dir).value.show_snapshots, "saved");

    // The next wizard starts with them shown; the toggle turns them off.
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('n'));
    h.settle();
    assert!(h.text().contains("26.4-snapshot-1"));
    h.click(&ui::Target::Key(KeyCode::Char(' ')));
    assert!(!h.text().contains("26.4-snapshot-1"));
    assert!(!Config::load(&h.data().dir).value.show_snapshots);
}

#[test]
fn filter_narrows_and_esc_steps_back() {
    let mut h = opened(100, 30);
    h.press(KeyCode::Char('/'));
    h.type_text("16");
    let text = h.text();
    assert!(
        text.contains("1.16.5") && !text.contains("1.21.8"),
        "{text}"
    );
    insta::assert_snapshot!("filtering", text);
    h.press(KeyCode::Enter);
    assert!(h.text().contains("1 of 4   “16”"));

    // Esc clears the filter, then leaves the wizard.
    h.press(KeyCode::Esc);
    assert!(h.text().contains("1.21.8"));
    h.press(KeyCode::Esc);
    assert!(h.text().contains("INSTANCES"), "back home");
}

#[test]
fn enter_picks_the_filtered_version() {
    let mut h = opened(100, 30);
    h.press(KeyCode::Char('/'));
    h.type_text("1.8");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    assert!(h.text().contains(&summary("Minecraft", "1.8.9")));
}

#[test]
fn fabric_offers_its_newest_stable_loader() {
    let mut h = opened(100, 30);
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    insta::assert_snapshot!("loader_checking", h.text());
    h.settle();
    insta::assert_snapshot!("loader_fabric", h.text());
}

#[test]
fn fabric_that_does_not_support_the_version_is_skipped() {
    let mut h = opened(100, 30);
    h.press(KeyCode::Enter);
    h.settle();
    insta::assert_snapshot!("loader_no_fabric", h.text());
    assert!(!h.press(KeyCode::Up), "Fabric can't be chosen");
    h.press(KeyCode::Enter);
    assert!(h.text().contains("26.3 Vanilla"), "on to the name step");
}

#[test]
fn coming_soon_loaders_are_not_selectable() {
    let mut h = opened(100, 30);
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    h.settle();
    for _ in 0..5 {
        h.press(KeyCode::Down);
    }
    h.click(&loader_row("forge"));
    assert_eq!(h.banner(), Some("Forge support is coming soon."));
    h.press(KeyCode::Enter);
    assert!(h.text().contains("1.21.8 Vanilla"), "stopped on Vanilla");
}

#[test]
fn name_step() {
    let mut h = at_name_step(&Loader::Fabric);
    insta::assert_snapshot!("name", h.text());
    h.press(KeyCode::Tab);
    assert!(h.text().contains("added on first launch"));
}

#[test]
fn a_typed_name_survives_going_back_and_forth() {
    let mut h = at_name_step(&Loader::Fabric);
    // An untouched name follows the choices.
    h.press(KeyCode::Esc);
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    assert!(h.text().contains("1.21.8 Vanilla"));

    h.type_text(" Hardcore");
    h.click(&step(Step::Version));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    let text = h.text();
    assert!(
        text.contains("1.21.8 Vanilla Hardcore") && text.contains(&summary("Minecraft", "1.16.5")),
        "{text}"
    );
}

#[test]
fn creates_a_vanilla_instance() {
    let mut h = at_name_step(&Loader::Vanilla);
    h.press(KeyCode::Enter);
    let inst = created(&h, "1.21.8 Vanilla");
    assert_eq!(inst.version, "1.21.8");
    assert_eq!(inst.loader, Loader::Vanilla);
    assert_eq!(inst.loader_version, "");
    assert!(!inst.install_starter_fabric_mods);
    assert_eq!(h.selected(), Some("1.21.8 Vanilla"));
    assert_eq!(h.banner(), Some("Created “1.21.8 Vanilla”."));
    assert!(h.text().contains("INSTANCES   4"));
}

#[test]
fn creates_a_fabric_instance_with_starter_mods() {
    let mut h = at_name_step(&Loader::Fabric);
    h.press(KeyCode::Char('!'));
    h.click(&ui::Target::Key(KeyCode::Enter));
    let inst = created(&h, "1.21.8 Fabric!");
    assert_eq!(inst.loader, Loader::Fabric);
    assert_eq!(inst.loader_version, "0.19.5");
    assert!(inst.install_starter_fabric_mods);
    assert_eq!(h.selected(), Some("1.21.8 Fabric!"));
}

#[test]
fn starter_mods_can_be_left_out() {
    let mut h = at_name_step(&Loader::Fabric);
    h.click(&ui::Target::Field(1));
    assert!(h.text().contains(&summary("Starter mods", "None")));
    h.press(KeyCode::Enter);
    assert!(!created(&h, "1.21.8 Fabric").install_starter_fabric_mods);
}

#[test]
fn names_must_not_be_empty() {
    let mut h = at_name_step(&Loader::Vanilla);
    for _ in 0.."1.21.8 Vanilla".len() {
        h.press(KeyCode::Backspace);
    }
    h.type_text("   ");
    h.press(KeyCode::Enter);
    assert!(h.text().contains("Give it a name."));
    assert_eq!(h.data().instances.len(), 3, "nothing created");
    assert_eq!(
        super::name_problem("a\u{7}b"),
        Some("Names can't contain control characters.")
    );
}

#[test]
fn suggestions_avoid_names_in_use() {
    let mut h = at_name_step(&Loader::Vanilla);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('n'));
    h.settle();
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    assert!(h.text().contains("1.21.8 Vanilla 2"));
}

#[test]
fn the_mouse_does_everything() {
    let mut h = opened(100, 30);
    // The wheel moves through versions, between rows too.
    let at = h.find(&version_row("26.3"));
    h.mouse(MouseEventKind::ScrollDown, at);
    h.mouse(MouseEventKind::ScrollDown, Position::new(at.x, at.y + 1));
    assert!(h.text().contains(&summary("Minecraft", "1.16.5")));

    // Double-click a version, then a loader.
    let at = h.find(&version_row("1.21.8"));
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    h.settle();
    assert!(h.text().contains("MOD LOADER"));
    let at = h.find(&loader_row("fabric"));
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    assert!(h.text().contains("YOUR INSTANCE"));

    // Finished steps are chips that go back.
    h.click(&step(Step::Loader));
    assert!(h.text().contains("MOD LOADER"));
    h.click(&step(Step::Version));
    assert!(h.text().contains("RELEASES"));
}

#[test]
fn never_panics_at_any_size() {
    let sizes = [
        (0, 0),
        (1, 1),
        (20, 6),
        (40, 10),
        (60, 16),
        (75, 20),
        (76, 12),
        (80, 24),
        (100, 8),
        (130, 34),
        (400, 60),
    ];
    for (w, hgt) in sizes {
        let mut h = opened(w, hgt);
        h.press(KeyCode::Char(' '));
        h.press(KeyCode::Char('/'));
        h.type_text("1.21");
        h.press(KeyCode::Enter);
        h.press(KeyCode::Enter);
        h.settle();
        h.press(KeyCode::Enter);
        h.press(KeyCode::Tab);
        h.text();
    }
}

/// A fuller version list than the fixture's, so screenshots look like the
/// real thing: id, kind, release day.
const SHOWCASE: &[(&str, &str, &str)] = &[
    ("26.4-snapshot-1", "snapshot", "2026-09-22"),
    ("26.3", "release", "2026-09-15"),
    ("26.3-rc-1", "snapshot", "2026-09-09"),
    ("26.2", "release", "2026-06-16"),
    ("26.1.2", "release", "2026-04-21"),
    ("26.1", "release", "2026-03-24"),
    ("1.21.11", "release", "2025-12-09"),
    ("1.21.10", "release", "2025-10-07"),
    ("1.21.9", "release", "2025-09-30"),
    ("1.21.8", "release", "2025-07-17"),
    ("1.21.7", "release", "2025-06-30"),
    ("1.21.6", "release", "2025-06-17"),
    ("1.21.5", "release", "2025-03-25"),
    ("1.21.4", "release", "2024-12-03"),
    ("1.21.1", "release", "2024-08-08"),
    ("1.20.6", "release", "2024-04-29"),
    ("1.20.1", "release", "2023-06-12"),
    ("1.19.4", "release", "2023-03-14"),
    ("1.18.2", "release", "2022-02-28"),
    ("1.16.5", "release", "2021-01-14"),
    ("1.12.2", "release", "2017-09-18"),
    ("1.8.9", "release", "2015-12-09"),
    ("b1.7.3", "old_beta", "2011-07-08"),
];

/// Home with Mojang serving [`SHOWCASE`] and Fabric supporting 1.21.8.
fn showcase(w: u16, hgt: u16) -> Harness {
    let versions: Vec<String> = SHOWCASE
        .iter()
        .map(|(id, kind, day)| {
            let at = format!("{day}T10:00:00+00:00");
            let sha1 = "0".repeat(40);
            format!(
                r#"{{"id":"{id}","type":"{kind}","url":"https://example.invalid/{id}.json","releaseTime":"{at}","sha1":"{sha1}"}}"#
            )
        })
        .collect();
    let manifest = format!(
        r#"{{"latest":{{"release":"26.3","snapshot":"26.4-snapshot-1"}},"versions":[{}]}}"#,
        versions.join(",")
    );
    let h = unanswered(w, hgt);
    let at = "/mc/game/version_manifest_v2.json";
    serve(&h, at, ResponseTemplate::new(200).set_body_string(manifest));
    serve_fabric(&h, "1.21.8");
    no_fabric(&h, "26.3");
    h
}

/// The wizard's states for `app::tests::screenshots`.
pub(crate) fn screenshots(save: &dyn Fn(&str, &mut Harness)) {
    let mut h = showcase(100, 30);
    h.press(KeyCode::Char('n'));
    h.settle();
    save("wizard-versions", &mut h);
    h.press(KeyCode::Char(' '));
    let at = h.find(&version_row("26.4-snapshot-1"));
    h.mouse(MouseEventKind::Moved, at);
    save("wizard-snapshots", &mut h);
    h.press(KeyCode::Char(' '));
    h.press(KeyCode::Char('/'));
    h.type_text("1.21");
    save("wizard-filter", &mut h);
    h.type_text(".8");
    h.press(KeyCode::Enter);
    save("wizard-filter-applied", &mut h);
    h.press(KeyCode::Enter);
    save("wizard-loader-checking", &mut h);
    h.settle();
    save("wizard-loader", &mut h);
    h.press(KeyCode::Enter);
    let at = h.find(&step(Step::Version));
    h.mouse(MouseEventKind::Moved, at);
    save("wizard-name", &mut h);
    h.press(KeyCode::Tab);
    save("wizard-name-starter", &mut h);
    h.press(KeyCode::Up);
    for _ in 0.."1.21.8 Fabric".len() {
        h.press(KeyCode::Backspace);
    }
    h.press(KeyCode::Enter);
    save("wizard-name-error", &mut h);

    let mut h = showcase(100, 30);
    h.press(KeyCode::Char('n'));
    h.settle();
    h.press(KeyCode::Enter);
    h.settle();
    save("wizard-loader-unsupported", &mut h);

    let mut h = unanswered(100, 30);
    h.press(KeyCode::Char('n'));
    save("wizard-loading", &mut h);
    h.settle();
    save("wizard-error", &mut h);

    let mut h = unanswered(100, 30);
    let cache = h.data().dir.cache_dir();
    std::fs::create_dir_all(&cache).unwrap();
    let saved = fixture("mojang/version_manifest_v2.json");
    std::fs::write(cache.join("version_manifest_v2.json"), saved).unwrap();
    h.press(KeyCode::Char('n'));
    h.settle();
    save("wizard-offline", &mut h);

    let mut h = showcase(64, 24);
    h.press(KeyCode::Char('n'));
    h.settle();
    save("wizard-compact-versions", &mut h);
    h.press(KeyCode::Char('/'));
    h.type_text("1.21.8");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.settle();
    h.press(KeyCode::Enter);
    save("wizard-compact", &mut h);

    let mut h = showcase(80, 24);
    h.press(KeyCode::Char('n'));
    h.settle();
    save("wizard-80x24", &mut h);
    h.press(KeyCode::Char('/'));
    h.type_text("1.21.8");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.settle();
    save("wizard-80x24-loader", &mut h);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Tab);
    save("wizard-80x24-name", &mut h);
}
