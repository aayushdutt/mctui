//! Vanilla Tweaks against real responses. The fixtures under
//! `tests/fixtures/vanilla_tweaks` are the site's own: catalogs trimmed from
//! the live 1.21 and 1.16 `rpcategories.json`, the build endpoint's answers
//! (a link, and the 400 it gives for unknown packs), and a cart the Go app
//! wrote.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mctui_core::facets;
use mctui_core::instance::{Instance, InstanceStore, NewInstance};
use mctui_core::net::Http;
use mctui_core::paths::DataDir;
use mctui_core::vanilla_tweaks::{
    self, Cart, CatalogVersion, Error, PackRef, PackState, Stage, Toggled, VanillaTweaks,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BUILD: &str = "/assets/server/zipresourcepacks.php";
/// Where `build-success.json` links to.
const DOWNLOAD: &str = "/download/VanillaTweaks_r397277_MC1.21.x.zip";
const ZIP: &[u8] = b"PK\x03\x04 the merged pack";
const OPTIONS: &str =
    "version:4189\nresourcePacks:[\"vanilla\",\"file/Faithful.zip\"]\nlang:en_us\n";

fn fixture(name: &str) -> Vec<u8> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vanilla_tweaks");
    fs::read(dir.join(name)).unwrap()
}

fn version(minecraft: &str) -> CatalogVersion {
    CatalogVersion::for_minecraft(minecraft).unwrap()
}

/// A scratch data dir holding one 1.21.4 instance.
struct Scratch {
    _tmp: tempfile::TempDir,
    data: DataDir,
    instance: Instance,
}

impl Scratch {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path());
        let instance = InstanceStore::new(&data)
            .create(NewInstance {
                name: "Tweaked".to_owned(),
                version: "1.21.4".to_owned(),
                ..NewInstance::default()
            })
            .unwrap();
        Self {
            _tmp: tmp,
            data,
            instance,
        }
    }

    fn game(&self) -> PathBuf {
        self.instance.game_dir()
    }

    fn client(&self, site: &MockServer) -> VanillaTweaks {
        VanillaTweaks::new(Http::new().unwrap(), &self.data).with_base_url(&site.uri())
    }

    fn options(&self) -> String {
        fs::read_to_string(self.game().join("options.txt")).unwrap()
    }

    /// Every file in the game dir, by relative path.
    fn files(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let rel = path.strip_prefix(root).unwrap().to_owned();
                    out.insert(rel, fs::read(&path).unwrap());
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&self.game(), &self.game(), &mut files);
        files
    }

    /// The game dir as a previous build left it: an old zip enabled in
    /// `options.txt`, and the Go app's cart.
    fn built_before(&self) {
        let packs = self.game().join("resourcepacks");
        fs::create_dir_all(&packs).unwrap();
        fs::write(packs.join("VanillaTweaks.zip"), "the old pack").unwrap();
        fs::write(
            packs.join(".mctui-vanillatweaks.json"),
            fixture("cart-go.json"),
        )
        .unwrap();
        fs::write(
            self.game().join("options.txt"),
            "resourcePacks:[\"vanilla\",\"file/VanillaTweaks.zip\"]\n",
        )
        .unwrap();
    }
}

/// A mock of the site serving the real 1.21 catalog.
async fn site() -> MockServer {
    let site = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/assets/resources/json/1.21/rpcategories.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture("rpcategories-1.21.json")))
        .mount(&site)
        .await;
    site
}

/// The build endpoint answering `status` with `body`, as the real one does:
/// JSON labelled as HTML.
async fn build_answers(site: &MockServer, status: u16, body: Vec<u8>) {
    Mock::given(method("POST"))
        .and(path(BUILD))
        .respond_with(ResponseTemplate::new(status).set_body_raw(body, "text/html; charset=UTF-8"))
        .mount(site)
        .await;
}

async fn serves_zip(site: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(DOWNLOAD))
        .respond_with(response)
        .mount(site)
        .await;
}

/// The fields of the one build request `site` received.
async fn build_request(site: &MockServer) -> Vec<(String, String)> {
    let requests = site.received_requests().await.unwrap();
    let builds: Vec<_> = requests.iter().filter(|r| r.url.path() == BUILD).collect();
    let [build] = builds.as_slice() else {
        panic!("want one build request, got {}", builds.len());
    };
    // Form bodies are encoded like a query string.
    let mut url = reqwest::Url::parse("http://form.invalid").unwrap();
    url.set_query(Some(std::str::from_utf8(&build.body).unwrap()));
    url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

#[tokio::test]
async fn decodes_the_real_1_21_catalog() {
    let site = site().await;
    let scratch = Scratch::new();
    let loaded = scratch
        .client(&site)
        .catalog(version("1.21.4"))
        .await
        .unwrap();
    assert!(loaded.notice.is_none());
    let catalog = loaded.value;
    assert_eq!(catalog.version.as_str(), "1.21");

    let top: Vec<_> = catalog.categories.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        top,
        [
            "Aesthetic",
            "Terrain",
            "Connected Textures",
            "GUI",
            "Retro",
            "Fun"
        ]
    );
    // Three levels deep: GUI › Hearts › LGBTQ+ Pride.
    let (pride, _) = catalog.pack("PrideHeartsTrans").unwrap();
    assert_eq!(pride.name, "LGBTQ+ Pride");
    let gui = &catalog.categories[3];
    assert_eq!(gui.subcategories[0].subcategories[0].name, "LGBTQ+ Pride");
    assert_eq!(gui.pack_count(), 6);

    let zombies = &catalog.categories[0].subcategories[0];
    assert_eq!(zombies.name, "More Zombies");
    let warning = zombies.warning.as_ref().unwrap();
    assert!(warning.color.starts_with("rgba("), "{}", warning.color);
    assert!(
        vanilla_tweaks::plain_text(&warning.text).contains("selecting all Zombie skins"),
        "{}",
        warning.text
    );

    let (_, pale) = catalog.pack("PalerPaleOak").unwrap();
    assert_eq!(pale.label(), "Paler Pale Oak");
    assert!(pale.experiment);
    assert_eq!(pale.priority, 1);
    assert_eq!(pale.preview_extension.as_deref(), Some("gif"));

    let (_, allay) = catalog.pack("AllayElytra").unwrap();
    assert_eq!(allay.video.as_deref(), Some("https://x.com/Ninnih_"));
    assert_eq!(allay.video_text.as_deref(), Some("Credit: Ninni"));
    assert_eq!(
        vanilla_tweaks::plain_text(&allay.description),
        "Changes Elytra to look like Allay wings. Credit: Ninni"
    );
    let (_, bacon) = catalog.pack("BaconBeam").unwrap();
    assert!(bacon.video.is_some() && bacon.video_text.is_none());

    // GoldenSavanna lists AlphaFoilage, which doesn't list it back.
    assert!(
        catalog
            .incompatible_with("AlphaFoilage")
            .contains("GoldenSavanna")
    );
    assert!(
        catalog
            .incompatible_with("GoldenSavanna")
            .contains("AlphaFoilage")
    );
}

#[tokio::test]
async fn decodes_the_real_1_16_catalog() {
    let site = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/assets/resources/json/1.16/rpcategories.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture("rpcategories-1.16.json")))
        .mount(&site)
        .await;
    let scratch = Scratch::new();
    let catalog = scratch
        .client(&site)
        .catalog(version("1.16.5"))
        .await
        .unwrap()
        .value;

    // 1.16 is flat, with no warnings, and some ids aren't plain words.
    assert!(
        catalog
            .categories
            .iter()
            .all(|c| c.subcategories.is_empty() && c.warning.is_none())
    );
    assert_eq!(catalog.packs().count(), 8);
    for id in [
        "3DDoors&Trapdoors",
        "VariatedBookshelves+ConnectedBookshelves",
        "1.11ShulkerBoxes",
    ] {
        assert!(catalog.pack(id).is_some(), "{id}");
    }
    assert!(
        catalog
            .incompatible_with("GlassDoors")
            .contains("3DDoors&Trapdoors")
    );
}

#[tokio::test]
async fn a_kept_catalog_is_served_offline() {
    let scratch = Scratch::new();
    let online = scratch
        .client(&site().await)
        .catalog(version("1.21"))
        .await
        .unwrap();

    // A site that answers nothing (404s, which aren't retried).
    let down = MockServer::start().await;
    let offline = scratch
        .client(&down)
        .catalog(version("1.21"))
        .await
        .unwrap();
    assert_eq!(offline.value, online.value);
    let notice = offline.notice.unwrap();
    assert!(
        notice.contains("showing the ones from your last visit"),
        "{notice}"
    );
}

#[tokio::test]
async fn no_catalog_without_the_site_or_a_kept_copy() {
    let down = MockServer::start().await;
    let err = Scratch::new()
        .client(&down)
        .catalog(version("1.21"))
        .await
        .unwrap_err();
    let Error::Net(err) = err else {
        panic!("want a network error, got {err:?}");
    };
    assert_eq!(err.status(), Some(reqwest::StatusCode::NOT_FOUND));
}

#[tokio::test]
async fn apply_builds_downloads_and_enables_the_pack() {
    let site = site().await;
    build_answers(&site, 200, fixture("build-success.json")).await;
    serves_zip(&site, ResponseTemplate::new(200).set_body_bytes(ZIP)).await;
    let scratch = Scratch::new();
    let game = scratch.game();
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join("options.txt"), OPTIONS).unwrap();
    let vt = scratch.client(&site);

    let catalog = vt.catalog(version("1.21.4")).await.unwrap().value;
    let mut cart = Cart::load(&game).await.value;
    cart.toggle(&catalog, "Aesthetic", "AllayElytra");
    cart.toggle(&catalog, "Terrain", "GoldenSavanna");
    // AlphaFoilage replaces GoldenSavanna, which lists it…
    let swapped = cart.toggle(&catalog, "Retro", "AlphaFoilage");
    let replaced = vec![PackRef {
        category: "Terrain".to_owned(),
        name: "GoldenSavanna".to_owned(),
    }];
    assert_eq!(swapped, Toggled::Added { replaced });
    // …and back.
    cart.toggle(&catalog, "Terrain", "GoldenSavanna");
    assert!(cart.has_unbuilt_changes());

    let mut stages = Vec::new();
    vt.apply_with(&game, catalog.version, &mut cart, |stage| {
        stages.push(stage);
    })
    .await
    .unwrap();

    assert_eq!(stages, [Stage::Building, Stage::Downloading]);
    assert_eq!(
        build_request(&site).await,
        [
            (
                "packs".to_owned(),
                r#"{"Aesthetic":["AllayElytra"],"Terrain":["GoldenSavanna"]}"#.to_owned()
            ),
            ("version".to_owned(), "1.21".to_owned()),
        ]
    );
    assert_eq!(
        fs::read(game.join("resourcepacks/VanillaTweaks.zip")).unwrap(),
        ZIP
    );
    let enabled = r#"resourcePacks:["vanilla","file/Faithful.zip","file/VanillaTweaks.zip"]"#;
    assert_eq!(
        scratch.options(),
        format!("version:4189\n{enabled}\nlang:en_us\n")
    );
    assert!(vanilla_tweaks::is_enabled(&game).await);

    assert!(!cart.has_unbuilt_changes());
    assert_eq!(cart.version, "1.21");
    assert_eq!(cart.state("Aesthetic", "AllayElytra"), PackState::Built);
    assert_eq!(Cart::load(&game).await.value, cart, "saved as applied");

    // The home screen lists the built packs.
    let listed: Vec<_> = facets::resource_packs(&scratch.instance)
        .into_iter()
        .map(|p| (p.name, p.category))
        .collect();
    assert_eq!(
        listed,
        [
            ("Allay Elytra".to_owned(), Some("Aesthetic".to_owned())),
            ("Golden Savanna".to_owned(), Some("Terrain".to_owned())),
        ]
    );
}

#[tokio::test]
async fn unapply_takes_the_pack_out_and_keeps_the_picks() {
    let scratch = Scratch::new();
    scratch.built_before();
    let game = scratch.game();
    fs::write(
        game.join("options.txt"),
        "resourcePacks:[\"vanilla\",\"file/VanillaTweaks.zip\",\"file/Top.zip\"]\n",
    )
    .unwrap();
    let mut cart = Cart::load(&game).await.value;
    let picks = cart.len();

    vanilla_tweaks::unapply(&game, &mut cart).await.unwrap();

    assert!(!game.join("resourcepacks/VanillaTweaks.zip").exists());
    assert_eq!(
        scratch.options(),
        "resourcePacks:[\"vanilla\",\"file/Top.zip\"]\n"
    );
    assert!(!vanilla_tweaks::is_enabled(&game).await);
    assert_eq!(cart.len(), picks);
    assert!(
        cart.entries()
            .all(|(_, state)| state == PackState::PendingAdd)
    );
    assert_eq!(Cart::load(&game).await.value, cart, "saved as unapplied");
    assert!(facets::resource_packs(&scratch.instance).is_empty());
}

#[tokio::test]
async fn applying_an_empty_cart_removes_the_pack() {
    let site = MockServer::start().await;
    let scratch = Scratch::new();
    scratch.built_before();
    let game = scratch.game();
    let mut cart = Cart::load(&game).await.value;
    cart.clear();

    scratch
        .client(&site)
        .apply(&game, version("1.21"), &mut cart)
        .await
        .unwrap();

    assert!(
        site.received_requests().await.unwrap().is_empty(),
        "no build"
    );
    assert!(!game.join("resourcepacks/VanillaTweaks.zip").exists());
    assert_eq!(scratch.options(), "resourcePacks:[\"vanilla\"]\n");
    assert!(!cart.has_unbuilt_changes());
    assert_eq!(cart.entries().count(), 0);
}

#[tokio::test]
async fn a_refused_build_changes_nothing() {
    let site = MockServer::start().await;
    build_answers(&site, 400, fixture("build-error.json")).await;
    let scratch = Scratch::new();
    scratch.built_before();
    let game = scratch.game();
    let mut cart = Cart::load(&game).await.value;
    let (files, before) = (scratch.files(), cart.clone());

    let err = scratch
        .client(&site)
        .apply(&game, version("1.21"), &mut cart)
        .await
        .unwrap_err();

    // The site's own words.
    assert!(matches!(&err, Error::Build(_)), "{err:?}");
    assert_eq!(
        err.to_string(),
        "Vanilla Tweaks couldn't build the pack: There was an error. Packs were invalid."
    );
    assert_eq!(scratch.files(), files);
    assert_eq!(cart, before);
}

#[tokio::test]
async fn a_download_link_off_the_site_is_refused() {
    let site = MockServer::start().await;
    let link = br#"{"status":"success","link":"https:\/\/elsewhere.example\/pack.zip"}"#;
    build_answers(&site, 200, link.to_vec()).await;
    let scratch = Scratch::new();
    scratch.built_before();
    let game = scratch.game();
    let mut cart = Cart::load(&game).await.value;
    let files = scratch.files();

    let err = scratch
        .client(&site)
        .apply(&game, version("1.21"), &mut cart)
        .await
        .unwrap_err();

    assert!(
        matches!(&err, Error::BadLink(link) if link == "https://elsewhere.example/pack.zip"),
        "{err:?}"
    );
    assert_eq!(scratch.files(), files);
}

#[tokio::test]
async fn cancelling_mid_download_changes_nothing() {
    let site = MockServer::start().await;
    build_answers(&site, 200, fixture("build-success.json")).await;
    let slow = ResponseTemplate::new(200)
        .set_body_bytes(ZIP)
        .set_delay(Duration::from_secs(30));
    serves_zip(&site, slow).await;
    let scratch = Scratch::new();
    scratch.built_before();
    let game = scratch.game();
    let mut cart = Cart::load(&game).await.value;
    let (files, before) = (scratch.files(), cart.clone());

    let vt = scratch.client(&site);
    let apply = vt.apply(&game, version("1.21"), &mut cart);
    // Dropping the future is how the UI cancels.
    let cancelled = tokio::time::timeout(Duration::from_millis(500), apply).await;

    assert!(cancelled.is_err(), "still downloading");
    let requests = site.received_requests().await.unwrap();
    assert!(
        requests.iter().any(|r| r.url.path() == DOWNLOAD),
        "cancelled mid-download"
    );
    assert_eq!(scratch.files(), files, "no partial zip, no edits");
    assert_eq!(cart, before);
}

#[tokio::test]
async fn reads_a_cart_the_go_app_wrote() {
    let scratch = Scratch::new();
    scratch.built_before();
    let game = scratch.game();

    let loaded = Cart::load(&game).await;
    assert!(loaded.notice.is_none());
    let cart = loaded.value;
    assert_eq!(cart.version, "1.21");
    let entries: Vec<_> = cart
        .entries()
        .map(|(pack, state)| (pack.category, pack.name, state))
        .collect();
    let row = |category: &str, name: &str, state| (category.to_owned(), name.to_owned(), state);
    assert_eq!(
        entries,
        [
            row("Aesthetic", "AllayElytra", PackState::Built),
            row("Aesthetic", "BlackNetherBricks", PackState::Built),
            row("Hearts", "ColoredHeartsWhite", PackState::PendingAdd),
            row("Terrain", "GoldenSavanna", PackState::PendingRemoval),
        ]
    );
    assert!(cart.has_unbuilt_changes());

    // Saved back in the same shape: sorted ids, same keys.
    cart.save(&game).await.unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(Cart::path(&game)).unwrap()).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&fixture("cart-go.json")).unwrap();
    assert_eq!(saved["version"], original["version"]);
    assert_eq!(saved["packs"]["Hearts"], original["packs"]["Hearts"]);
    assert_eq!(
        saved["applied"]["Aesthetic"],
        serde_json::json!(["AllayElytra", "BlackNetherBricks"])
    );
    assert_eq!(saved["applied"]["Terrain"], original["applied"]["Terrain"]);
}

#[tokio::test]
async fn preview_links() {
    let scratch = Scratch::new();
    let catalog = scratch
        .client(&site().await)
        .catalog(version("1.21"))
        .await
        .unwrap()
        .value;
    let live = VanillaTweaks::new(Http::new().unwrap(), &scratch.data);
    let preview = |id| live.preview_url(catalog.version, catalog.pack(id).unwrap().1);
    let previews = "https://vanillatweaks.net/assets/resources/previews/resourcepacks/1.21";
    assert_eq!(
        preview("PalerPaleOak"),
        format!("{previews}/PalerPaleOak.gif"),
        "animated"
    );
    assert_eq!(
        preview("BlackNetherBricks"),
        format!("{previews}/BlackNetherBricks.png")
    );
}

/// Fetches the real 1.21 catalog. Run with `cargo test -- --ignored`.
#[tokio::test]
#[ignore = "talks to vanillatweaks.net"]
async fn live_catalog() {
    let scratch = Scratch::new();
    let vt = VanillaTweaks::new(Http::new().unwrap(), &scratch.data);
    let loaded = vt.catalog(version("1.21.4")).await.unwrap();
    assert!(loaded.notice.is_none());
    let catalog = loaded.value;
    assert!(catalog.packs().count() > 100, "{}", catalog.packs().count());
    for id in ["BlackNetherBricks", "AllayElytra", "PrideHeartsTrans"] {
        assert!(catalog.pack(id).is_some(), "{id}");
    }
}
