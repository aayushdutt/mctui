//! The resource packs screen end to end, against a mock of Vanilla Tweaks
//! serving the site's real responses (the core crate's fixtures).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use mctui_core::vanilla_tweaks::{Cart, PackState};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::browser::{CART, CATEGORIES, PACKS, RESULTS};
use super::{Command, Pane, TAB, target};
use crate::app::tests::Harness;

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../../mctui-core/tests/fixtures/vanilla_tweaks/",
            $name
        ))
    };
}

const CATALOG: &[u8] = fixture!("rpcategories-1.21.json");
/// A cart the Go app wrote: Allay Elytra and Black Nether Bricks built,
/// White Hearts picked since, Golden Savanna built but unpicked since.
const GO_CART: &[u8] = fixture!("cart-go.json");
const BUILT: &[u8] = fixture!("build-success.json");
const REFUSED: &[u8] = fixture!("build-error.json");
const BUILD: &str = "/assets/server/zipresourcepacks.php";
/// Where `build-success.json` links to.
const DOWNLOAD: &str = "/download/VanillaTweaks_r397277_MC1.21.x.zip";
const ZIP: &[u8] = b"PK\x03\x04 the merged pack";
const ENABLED: &str = "resourcePacks:[\"vanilla\",\"file/VanillaTweaks.zip\"]\n";

/// The site serving the real 1.21 catalog.
fn catalog() -> Mock {
    Mock::given(method("GET"))
        .and(path("/assets/resources/json/1.21/rpcategories.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(CATALOG))
}

/// The build endpoint answering `status` with `body` after `delay`, as the
/// real one does (JSON labelled as HTML), and the zip it links to.
fn serve_builds(h: &Harness, status: u16, body: &[u8], delay: Duration) {
    h.mock(
        Mock::given(method("POST")).and(path(BUILD)).respond_with(
            ResponseTemplate::new(status)
                .set_body_raw(body.to_vec(), "text/html; charset=UTF-8")
                .set_delay(delay),
        ),
    );
    h.mock(
        Mock::given(method("GET"))
            .and(path(DOWNLOAD))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(ZIP)),
    );
}

/// The sample data with "Survival 1.21" (1.21.4) set up by `prepare`, and a
/// mock site that answers nothing yet.
fn harness((w, h): (u16, u16), prepare: impl FnOnce(&Path)) -> Harness {
    Harness::online(w, h, |data| prepare(&game(data)))
}

/// Opens the selected instance's resource packs and waits for them.
fn open_packs(h: &mut Harness) {
    h.press(KeyCode::Char('p'));
    h.settle();
}

/// The resource packs of "Survival 1.21", loaded from the mock site.
fn open(screen: (u16, u16), prepare: impl FnOnce(&Path)) -> Harness {
    let mut h = harness(screen, prepare);
    h.mock(catalog());
    open_packs(&mut h);
    h
}

fn game(data: &crate::data::Data) -> PathBuf {
    data.instance("Survival 1.21").unwrap().game_dir()
}

fn fresh(_: &Path) {}

/// The game dir as a previous build left it: a zip enabled in `options.txt`,
/// and the Go app's cart.
fn built_before(game: &Path) {
    let packs = game.join("resourcepacks");
    fs::create_dir_all(&packs).unwrap();
    fs::write(packs.join("VanillaTweaks.zip"), "the old pack").unwrap();
    fs::write(packs.join(".mctui-vanillatweaks.json"), GO_CART).unwrap();
    fs::write(game.join("options.txt"), ENABLED).unwrap();
}

/// The cart as saved in `game`.
fn saved_cart(game: &Path) -> Cart {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(Cart::load(game)).value
}

fn press_all(h: &mut Harness, keys: &[KeyCode]) {
    for key in keys {
        h.press(*key);
    }
}

/// Waits for background work (saves) to land.
fn eventually(mut done: impl FnMut() -> bool) {
    for _ in 0..300 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting");
}

fn row(list: &'static str, key: &str) -> crate::ui::Target {
    crate::ui::Target::Row {
        list,
        key: key.to_owned(),
    }
}

#[test]
fn opens_on_the_catalog() {
    let mut h = open((120, 32), fresh);
    insta::assert_snapshot!(h.text());
}

#[test]
fn packs_show_their_state() {
    let mut h = open((120, 32), built_before);
    // Aesthetic: pick Paler Pale Oak, unpick Black Nether Bricks.
    press_all(
        &mut h,
        &[
            KeyCode::Tab,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Up,
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
        ],
    );
    let text = h.text();
    insta::assert_snapshot!(text);
    for state in [
        "● built",
        "● adding",
        "○ removing",
        "⇄ replaces Allay Elytra",
    ] {
        assert!(text.contains(state), "{state}: {text}");
    }
}

#[test]
fn your_pack_groups_by_category() {
    let mut h = open((120, 32), built_before);
    press_all(&mut h, &[KeyCode::Tab, KeyCode::Tab]);
    insta::assert_snapshot!(h.text());
    // ↑ from the first pack stays put: the heading above isn't a stop.
    assert!(!h.press(KeyCode::Up));
    // Space keeps a pack that was on its way out.
    h.click(&row(CART, "GoldenSavanna"));
    h.press(KeyCode::Char(' '));
    assert!(!h.text().contains("○ removing"));
    h.click(&row(CART, "AllayElytra"));
    // ↵ shows the pack where it lives.
    press_all(&mut h, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let text = h.text();
    assert!(text.contains("GUI › HEARTS"), "{text}");
}

#[test]
fn search_finds_packs_everywhere() {
    let mut h = open((120, 32), fresh);
    h.press(KeyCode::Char('/'));
    h.type_text("elytra");
    let text = h.text();
    insta::assert_snapshot!(text);
    assert!(
        text.contains("2 packs") && text.contains("Vex Elytra"),
        "{text}"
    );

    // ↵ keeps the results; space then picks one.
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char(' '));
    assert!(h.text().contains("● adding"));
    // Clicking a result stops typing too.
    h.press(KeyCode::Char('/'));
    h.click(&row(RESULTS, "ConcorpWings"));
    h.press(KeyCode::Char(' '));
    assert_eq!(
        h.banner(),
        Some("Swapped in Vex Elytra, replacing Allay Elytra.")
    );
    // esc leaves the search, then the screen.
    h.press(KeyCode::Esc);
    assert!(h.text().contains("AESTHETIC"));
    h.press(KeyCode::Esc);
    assert!(h.text().contains("INSTANCES"), "back home");
}

#[test]
fn toggling_saves_and_swaps_conflicts() {
    let mut h = open((120, 32), fresh);
    let game = game(h.data());
    // GUI: Dark UI, then Transparent UI, which can't be used with it.
    press_all(
        &mut h,
        &[
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Tab,
            KeyCode::Enter,
        ],
    );
    assert!(h.text().contains("⇄ replaces Dark UI"));
    h.press(KeyCode::Down);
    h.press(KeyCode::Char(' '));
    assert_eq!(
        h.banner(),
        Some("Swapped in Transparent UI, replacing Dark UI.")
    );
    eventually(|| saved_cart(&game).contains("GUI", "TransparentUI"));
    let cart = saved_cart(&game);
    assert_eq!(cart.len(), 1, "Dark UI went");
    assert_eq!(cart.state("GUI", "TransparentUI"), PackState::PendingAdd);
}

#[test]
fn a_pick_made_just_before_closing_is_still_saved() {
    let mut h = open((120, 32), fresh);
    let game = game(h.data());
    press_all(
        &mut h,
        &[
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Tab,
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Esc,
        ],
    );
    assert!(h.text().contains("INSTANCES"), "closed at once");
    eventually(|| saved_cart(&game).contains("GUI", "DarkUI"));
}

#[test]
fn builds_and_applies_the_pack() {
    let mut h = open((120, 32), |game| {
        fs::create_dir_all(game).unwrap();
        fs::write(game.join("options.txt"), "lang:en_us\n").unwrap();
    });
    serve_builds(&h, 200, BUILT, Duration::ZERO);
    let game = game(h.data());
    // Allay Elytra, then Golden Savanna from Terrain.
    press_all(
        &mut h,
        &[
            KeyCode::Tab,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::BackTab,
            KeyCode::Down,
            KeyCode::Tab,
            KeyCode::Down,
            KeyCode::Char(' '),
        ],
    );
    assert!(h.text().contains("● unbuilt changes"));
    h.press(KeyCode::Char('b'));
    assert!(h.text().contains("Building your pack…"));
    h.settle();

    assert_eq!(h.banner(), Some("Applied 2 tweaks."));
    assert!(h.text().contains("✓ applied"));
    assert_eq!(h.requests(BUILD), 1);
    assert_eq!(
        fs::read(game.join("resourcepacks/VanillaTweaks.zip")).unwrap(),
        ZIP
    );
    let options = fs::read_to_string(game.join("options.txt")).unwrap();
    assert!(options.contains("\"file/VanillaTweaks.zip\""), "{options}");
    let cart = saved_cart(&game);
    assert_eq!(cart.state("Aesthetic", "AllayElytra"), PackState::Built);
    assert_eq!(cart.state("Terrain", "GoldenSavanna"), PackState::Built);
}

#[test]
fn building_shows_progress() {
    let mut h = open((120, 32), built_before);
    serve_builds(&h, 200, BUILT, Duration::from_secs(30));
    h.press(KeyCode::Char('b'));
    insta::assert_snapshot!(h.text());
    // Edits wait; esc stops the build and leaves everything as it was.
    h.press(KeyCode::Tab);
    h.press(KeyCode::Char(' '));
    assert_eq!(
        h.banner(),
        Some("Wait for your pack to finish, or press esc to stop it.")
    );
    h.press(KeyCode::Esc);
    assert_eq!(
        h.banner(),
        Some("Stopped. Your pack in the game is as it was.")
    );
    assert!(h.text().contains("Build & apply"));
}

#[test]
fn a_refused_build_says_why() {
    let mut h = open((120, 32), built_before);
    serve_builds(&h, 400, REFUSED, Duration::ZERO);
    h.press(KeyCode::Char('b'));
    h.settle();
    assert_eq!(
        h.banner(),
        Some("Vanilla Tweaks couldn't build the pack: There was an error. Packs were invalid.")
    );
    assert!(h.text().contains("● unbuilt changes"));
}

#[test]
fn clearing_asks_first() {
    let mut h = open((120, 32), built_before);
    let game = game(h.data());
    h.press(KeyCode::Char('c'));
    let text = h.text();
    assert!(text.contains("Clear your pack?"), "{text}");
    h.press(KeyCode::Enter);
    assert!(h.text().contains("YOUR PACK   3"), "↵ keeps it");

    h.press(KeyCode::Char('c'));
    h.press(KeyCode::Left);
    h.press(KeyCode::Enter);
    let text = h.text();
    assert!(!text.contains("YOUR PACK   3"), "{text}");
    assert!(
        text.contains("○ removing"),
        "built packs go at the next build"
    );
    eventually(|| saved_cart(&game).is_empty());
}

#[test]
fn removes_the_pack_from_the_game() {
    let mut h = open((120, 32), built_before);
    let game = game(h.data());
    h.press(KeyCode::Char('u'));
    assert!(h.text().contains("Remove your pack from the game?"));
    h.press(KeyCode::Char('y'));
    h.settle();
    assert_eq!(
        h.banner(),
        Some("Removed your pack from “Survival 1.21”. Your picks are still here.")
    );
    assert!(!game.join("resourcepacks/VanillaTweaks.zip").exists());
    assert_eq!(
        fs::read_to_string(game.join("options.txt")).unwrap(),
        "resourcePacks:[\"vanilla\"]\n"
    );
    let cart = saved_cart(&game);
    assert_eq!(cart.len(), 3, "the picks stay");
    assert!(
        cart.entries()
            .all(|(_, state)| state == PackState::PendingAdd)
    );
}

#[test]
fn says_when_the_game_is_not_using_the_pack() {
    let mut h = open((120, 32), |game| {
        built_before(game);
        fs::write(game.join("options.txt"), "resourcePacks:[\"vanilla\"]\n").unwrap();
    });
    let text = h.text();
    assert!(text.contains("Minecraft isn't using your pack"), "{text}");
}

#[test]
fn unsupported_versions_say_why() {
    let mut h = harness((100, 30), fresh);
    h.press(KeyCode::Down);
    h.press(KeyCode::Char('p'));
    let text = h.text();
    insta::assert_snapshot!(text);
    assert!(text.contains("make packs for snapshots"), "{text}");
    h.press(KeyCode::Esc);
    assert!(h.text().contains("INSTANCES"));
}

#[test]
fn offline_shows_the_last_catalog() {
    let mut h = harness((120, 32), fresh);
    // The site answers once, then goes away.
    h.mock(catalog().up_to_n_times(1));
    open_packs(&mut h);
    h.press(KeyCode::Esc);
    open_packs(&mut h);
    let text = h.text();
    insta::assert_snapshot!(text);
    assert!(text.contains("packs from your last visit"), "{text}");

    // r tries again; the site is back.
    h.mock(catalog());
    h.press(KeyCode::Char('r'));
    h.settle();
    assert_eq!(h.banner(), Some("Back online: these are the latest packs."));
    assert!(!h.text().contains("last visit"));
}

#[test]
fn a_failed_load_can_be_retried() {
    let mut h = harness((100, 30), fresh);
    open_packs(&mut h);
    let text = h.text();
    assert!(text.contains("Couldn't load the packs"), "{text}");
    assert!(text.contains("answered 404 Not Found"), "{text}");
    h.mock(catalog());
    h.press(KeyCode::Char('r'));
    h.settle();
    assert!(h.text().contains("CATEGORIES"));
}

#[test]
fn narrow_terminals_show_fewer_panes() {
    let mut h = open((90, 28), built_before);
    press_all(&mut h, &[KeyCode::Tab, KeyCode::Tab]);
    insta::assert_snapshot!("split_your_pack", h.text());

    let mut h = open((70, 28), built_before);
    h.press(KeyCode::Tab);
    let text = h.text();
    insta::assert_snapshot!("single_packs", text);
    assert!(!text.contains("CATEGORIES"), "one pane at a time");
    h.click(&target(TAB, Pane::Cart.key()));
    assert!(h.text().contains("White Hearts"), "the tab shows your pack");
}

#[test]
fn the_tree_opens_and_closes() {
    let mut h = open((120, 32), fresh);
    assert!(!h.text().contains("More Zombies"));
    h.press(KeyCode::Right);
    assert!(h.text().contains("More Zombies"), "→ opens");
    h.press(KeyCode::Down);
    assert!(h.text().contains("AESTHETIC › MORE ZOMBIES"));
    h.press(KeyCode::Left);
    assert!(h.text().contains("AESTHETIC   5"), "← goes to the parent");
    h.press(KeyCode::Left);
    assert!(!h.text().contains("More Zombies"), "← closes");
    // → on an open category with packs of its own moves on to them.
    press_all(
        &mut h,
        &[KeyCode::Right, KeyCode::Right, KeyCode::Char(' ')],
    );
    assert!(h.text().contains("● adding"));
}

#[test]
fn mouse_selects_toggles_and_opens_menus() {
    let mut h = open((120, 32), fresh);
    serve_builds(&h, 200, BUILT, Duration::from_secs(30));
    h.click(&row(CATEGORIES, "GUI"));
    assert!(h.text().contains("Hearts"), "a click opens a category");

    // A double-click adds a pack.
    let at = h.find(&row(PACKS, "DarkUI"));
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    assert!(h.text().contains("● adding"));

    // Right-click: a menu at the pointer, whose keys run its entries.
    let at = h.find(&row(PACKS, "TransparentUI"));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    let text = h.text();
    insta::assert_snapshot!(text);
    assert!(text.contains("Add to your pack"), "{text}");
    h.press(KeyCode::Char(' '));
    assert_eq!(
        h.banner(),
        Some("Swapped in Transparent UI, replacing Dark UI.")
    );

    // The wheel moves the list under the pointer.
    let at = h.find(&row(CATEGORIES, "Aesthetic"));
    h.mouse(MouseEventKind::ScrollDown, at);
    assert!(
        h.text().contains("GUI › HEARTS"),
        "from GUI to the next row"
    );

    // Buttons act once, from anywhere.
    h.click(&Command::Build.button());
    assert!(h.text().contains("Building your pack…"));
}

#[test]
fn previews_and_videos_open_in_the_browser() {
    let mut h = open((120, 32), fresh);
    press_all(&mut h, &[KeyCode::Tab, KeyCode::Down, KeyCode::Down]);
    h.press(KeyCode::Char('o'));
    let opened = h.opened();
    let [preview] = opened.as_slice() else {
        panic!("{opened:?}");
    };
    assert!(
        preview.ends_with("/assets/resources/previews/resourcepacks/1.21/PalerPaleOak.gif"),
        "{preview}"
    );
    assert!(!h.press(KeyCode::Char('w')), "no video for this one");
    h.press(KeyCode::Down);
    h.click(&Command::Video.button());
    assert_eq!(h.opened(), ["https://x.com/Ninnih_"]);
    assert_eq!(h.banner(), Some("Opened the video for Allay Elytra."));
}

#[test]
fn every_state_fits_any_size() {
    for (w, hgt) in [(40, 12), (60, 20), (80, 24), (99, 30), (100, 16), (160, 50)] {
        let mut h = open((w, hgt), built_before);
        for key in [
            KeyCode::Tab,
            KeyCode::Char(' '),
            KeyCode::Tab,
            KeyCode::Down,
            KeyCode::Char('/'),
            KeyCode::Char('x'),
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Left,
        ] {
            h.press(key);
        }
    }
}

/// Renders the screen's states for the app's `screenshots` test.
pub(crate) fn screenshots(save: &dyn Fn(&str, &mut Harness)) {
    save("packs", &mut open((120, 32), fresh));

    let mut h = open((120, 32), built_before);
    serve_builds(&h, 200, BUILT, Duration::from_secs(30));
    press_all(
        &mut h,
        &[
            KeyCode::Tab,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Up,
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
        ],
    );
    save("packs-states", &mut h);
    press_all(&mut h, &[KeyCode::Tab]);
    save("packs-cart", &mut h);
    h.press(KeyCode::Char('/'));
    h.type_text("hearts");
    save("packs-search", &mut h);
    press_all(&mut h, &[KeyCode::Esc, KeyCode::Char('b')]);
    save("packs-building", &mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('c'));
    save("packs-clear", &mut h);
    h.press(KeyCode::Esc);
    let at = h.find(&row(CATEGORIES, "GUI"));
    h.mouse(MouseEventKind::Moved, at);
    save("packs-hover", &mut h);
    let at = h.find(&row(PACKS, "AllayElytra"));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    save("packs-menu", &mut h);
    press_all(
        &mut h,
        &[
            KeyCode::Esc,
            KeyCode::BackTab,
            KeyCode::Right,
            KeyCode::Down,
            KeyCode::Tab,
        ],
    );
    save("packs-warning", &mut h);

    save("packs-split", &mut open((90, 28), built_before));
    let mut single = open((70, 28), built_before);
    single.press(KeyCode::Tab);
    save("packs-single", &mut single);

    let mut h = harness((100, 30), fresh);
    h.press(KeyCode::Down);
    h.press(KeyCode::Char('p'));
    save("packs-unsupported", &mut h);

    let mut h = harness((100, 30), fresh);
    open_packs(&mut h);
    save("packs-failed", &mut h);

    let mut h = harness((120, 32), fresh);
    h.mock(catalog().up_to_n_times(1));
    open_packs(&mut h);
    h.press(KeyCode::Esc);
    open_packs(&mut h);
    save("packs-offline", &mut h);

    let mut h = harness((100, 30), fresh);
    h.mock(catalog());
    h.press(KeyCode::Char('p'));
    save("packs-loading", &mut h);
}
