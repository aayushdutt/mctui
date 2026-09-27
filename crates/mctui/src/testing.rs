//! Test fixtures: a sample data folder at a fixed "now", and helpers to render
//! frames to text or ANSI (for screenshots).

use std::fmt::Write as _;
use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use mctui_core::account::{Account, AccountKind, Accounts, Secret};
use mctui_core::auth::{Auth, Endpoints};
use mctui_core::config::Config;
use mctui_core::fabric::Fabric;
use mctui_core::instance::{Instance, InstanceStore, Loader};
use mctui_core::modrinth;
use mctui_core::mojang::Mojang;
use mctui_core::net::Http;
use mctui_core::paths::DataDir;
use mctui_core::vanilla_tweaks::VanillaTweaks;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use tempfile::TempDir;

use crate::data::Data;
use crate::services::Services;

/// The fixed present every fixture and frame is measured against.
pub fn now() -> Timestamp {
    "2026-09-27T12:00:00Z".parse().expect("valid timestamp")
}

pub fn tz() -> TimeZone {
    TimeZone::UTC
}

fn ago(secs: i64) -> Timestamp {
    now() - jiff::SignedDuration::from_secs(secs)
}

fn set_mtime(path: &Path, secs_ago: u64) {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(now().as_second().unsigned_abs());
    File::open(path)
        .and_then(|f| f.set_modified(now - Duration::from_secs(secs_ago)))
        .expect("set mtime");
}

/// Sample data in a temp folder. Keep the `TempDir` alive while using `data`.
pub struct Fixture {
    pub dir: TempDir,
    pub data: Data,
}

pub fn fixture() -> Fixture {
    let dir = TempDir::new().expect("temp dir");
    let data_dir = DataDir::new(dir.path());
    let store = InstanceStore::new(&data_dir);

    let survival = instance(&data_dir, "Survival 1.21", |i| {
        i.version = "1.21.4".into();
        i.loader = Loader::Fabric;
        i.loader_version = "0.16.10".into();
        i.last_played = Some(ago(2 * 3600));
        i.created_at = Some(ago(30 * 86_400));
        i.play_time_secs = 4 * 3600 + 12 * 60;
        i.is_fully_downloaded = true;
    });
    let game = survival.game_dir();
    for (world, secs) in [("New World", 2 * 3600), ("Hardcore Run", 3 * 86_400)] {
        let path = game.join("saves").join(world);
        fs::create_dir_all(&path).expect("mkdir");
        set_mtime(&path, secs);
    }
    fs::create_dir_all(game.join("mods")).expect("mkdir");
    for (jar, size) in [
        ("sodium-fabric-0.6.0+mc1.21.4.jar", 1_258_291),
        ("lithium-fabric-0.14.3+mc1.21.4.jar", 700_000),
        ("fabric-api-0.110.0+1.21.4.jar", 2_100_000),
    ] {
        File::create(game.join("mods").join(jar))
            .and_then(|f| f.set_len(size))
            .expect("jar");
    }

    instance(&data_dir, "Creative Build", |i| {
        i.version = "1.21.1".into();
        i.last_played = Some(ago(3 * 86_400));
        i.play_time_secs = 50 * 60;
        i.is_fully_downloaded = true;
    });
    instance(&data_dir, "Snapshot Test", |i| {
        i.version = "25w14a".into();
        i.loader = Loader::Fabric;
        i.created_at = Some(ago(5 * 3600));
    });

    let mut accounts = Accounts::default();
    accounts.upsert(Account {
        id: "069a79f444e94726a5befca90e38aaf5".into(),
        name: "Steve".into(),
        kind: AccountKind::Msa,
        access_token: Secret::new("test-token"),
        expires_at: Some(ago(-86_400)),
        msa_refresh_token: None,
    });

    let instances = store.scan().expect("scan").instances;
    let data = Data {
        dir: data_dir.clone(),
        config: Config::default(),
        accounts,
        sessions: std::collections::HashMap::new(),
        store,
        instances,
    };
    Fixture { dir, data }
}

/// Clients that never reach the real services: sign-in talks to `base` (a
/// mock server) and polls in milliseconds, or, without one, to a port
/// nothing listens on.
pub fn services(base: Option<String>, data: &DataDir) -> Services {
    let http = Http::new().expect("http client");
    let base = base.unwrap_or_else(|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        format!("http://{}", listener.local_addr().expect("address"))
    });
    let endpoints = Endpoints {
        device_code: format!("{base}/devicecode"),
        token: format!("{base}/token"),
        xbox_user: format!("{base}/user/authenticate"),
        xsts: format!("{base}/xsts/authorize"),
        minecraft_login: format!("{base}/authentication/login_with_xbox"),
        minecraft_profile: format!("{base}/minecraft/profile"),
    };
    let mut services = Services::new(http.clone(), data);
    services.auth =
        Auth::with_endpoints(http.clone(), endpoints).with_poll_unit(Duration::from_millis(1));
    services.mojang = Mojang::with_manifest_url(
        http.clone(),
        data,
        format!("{base}/mc/game/version_manifest_v2.json"),
    );
    services.fabric = Fabric::with_meta_url(http.clone(), data, base.clone());
    services.modrinth = modrinth::Client::with_base_url(http.clone(), base.clone());
    services.vanilla_tweaks = VanillaTweaks::new(http, data).with_base_url(&base);
    services
}

/// An instance folder on disk, with its `Instance`.
fn instance(dir: &DataDir, name: &str, edit: impl FnOnce(&mut Instance)) -> Instance {
    let path = dir.instances_dir().join(name);
    fs::create_dir_all(path.join(".minecraft")).expect("mkdir");
    let mut inst = Instance {
        id: name.into(),
        name: name.into(),
        path,
        ..Instance::default()
    };
    edit(&mut inst);
    InstanceStore::new(dir).save(&inst).expect("save instance");
    inst
}

/// The frame as plain text, trailing spaces trimmed.
pub fn text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        out.push_str(row.trim_end());
        out.push('\n');
    }
    out
}

/// The frame as truecolor ANSI, for `scripts/ansi2png.py`.
pub fn ansi(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut last = None;
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            let style = (cell.fg, cell.bg, cell.modifier);
            if last != Some(style) {
                out.push_str("\x1b[0");
                if let Color::Rgb(r, g, b) = cell.fg {
                    let _ = write!(out, ";38;2;{r};{g};{b}");
                }
                if let Color::Rgb(r, g, b) = cell.bg {
                    let _ = write!(out, ";48;2;{r};{g};{b}");
                }
                for (m, code) in [
                    (Modifier::BOLD, "1"),
                    (Modifier::DIM, "2"),
                    (Modifier::ITALIC, "3"),
                    (Modifier::UNDERLINED, "4"),
                ] {
                    if cell.modifier.contains(m) {
                        out.push(';');
                        out.push_str(code);
                    }
                }
                out.push('m');
                last = Some(style);
            }
            out.push_str(cell.symbol());
        }
        out.push_str("\x1b[0m\n");
    }
    out
}
