//! Compatibility with files the Go app wrote. The fixtures under
//! `tests/fixtures/data` mirror real Go output (times with offsets, `null`
//! `jvmArgs`, zero times, a missing `createdAt`), minus personal data.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use mctui_core::account::{AccountKind, Accounts, Secret};
use mctui_core::config::{Config, LogVerbosity};
use mctui_core::instance::{InstanceStore, Loader};
use mctui_core::paths::DataDir;

/// The data dir the fixtures were "written" in.
const FIXTURE_ROOT: &str = "/home/steve/.local/share/mctui";

/// A scratch copy of the fixture data dir, so tests can write to it. Its
/// `config.json` names the copy as its data dir, as Go would have written it.
fn fixture_data() -> (tempfile::TempDir, DataDir) {
    let tmp = tempfile::tempdir().unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    copy_dir(&src, tmp.path());
    let dir = DataDir::new(tmp.path());
    let config = fixture_text("config.json").replace(FIXTURE_ROOT, &tmp.path().to_string_lossy());
    fs::write(dir.config_file(), config).unwrap();
    (tmp, dir)
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), to).unwrap();
        }
    }
}

fn fixture_text(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/data")
        .join(rel);
    fs::read_to_string(path).unwrap()
}

/// The top-level keys of a pretty-printed JSON object, in file order.
fn top_level_keys(json: &str) -> Vec<String> {
    json.lines()
        .filter_map(|line| line.strip_prefix("  \""))
        .filter_map(|rest| rest.split_once("\":"))
        .map(|(key, _)| key.to_owned())
        .collect()
}

#[test]
fn go_instances_scan() {
    let (_tmp, dir) = fixture_data();
    let scan = InstanceStore::new(&dir).scan().unwrap();

    let ids: Vec<&str> = scan.instances.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(
        ids,
        ["Vanilla", "1.21.6 Fabric", "Old Snapshot", "Fresh"],
        "by recency"
    );

    // The folder, not the file, decides id and path.
    let old = &scan.instances[2];
    assert_eq!(old.name, "Old Snapshot");
    assert_eq!(old.path, dir.instances_dir().join("Old Snapshot"));
    assert_eq!(old.created_at, None, "missing createdAt");
    assert!(old.last_played.is_some());

    let fabric = &scan.instances[1];
    assert_eq!(fabric.loader, Loader::Fabric);
    assert_eq!(fabric.loader_version, "0.19.3");
    assert!(fabric.jvm_args.is_empty(), "null jvmArgs");
    assert_eq!(fabric.created_at, None, "zero createdAt");
    assert_eq!(
        fabric.last_played,
        Some("2026-06-15T14:07:10.324669Z".parse().unwrap())
    );
    assert_eq!(fabric.play_time_secs, 7091);
    assert_eq!(fabric.download_key(), fabric.download_cache_key);
    assert_eq!(
        fabric.java_path.as_deref(),
        Some(Path::new("/home/steve/.config/mctui/java/21/bin/java"))
    );

    let vanilla = &scan.instances[0];
    assert_eq!(vanilla.loader, Loader::Vanilla);
    assert_eq!(vanilla.java_path, None);
    assert_eq!(vanilla.jvm_args, ["-Xmx3G"]);
    assert_eq!(vanilla.last_played, None);
    assert_eq!(vanilla.cached_at, None);

    let fresh = &scan.instances[3];
    assert_eq!(
        fresh.loader,
        Loader::Fabric,
        "loader ids are case-insensitive"
    );
    assert_eq!(fresh.play_time_secs, 0, "negative playTime");
    assert!(fresh.install_starter_fabric_mods);
    assert_eq!(fresh.recency(), None);

    // A malformed file is reported; a folder without instance.json isn't.
    let problems: Vec<PathBuf> = scan.problems.iter().map(|p| p.dir.clone()).collect();
    assert_eq!(problems, [dir.instances_dir().join("Broken")]);
    assert!(!scan.problems[0].error.is_empty());
}

#[test]
fn instances_round_trip() {
    let (_tmp, dir) = fixture_data();
    let store = InstanceStore::new(&dir);
    let before = store.scan().unwrap().instances;
    for inst in &before {
        store.save(inst).unwrap();
    }
    let after = store.scan().unwrap().instances;
    assert_eq!(before, after);
}

#[test]
fn saved_instance_is_byte_identical_to_go() {
    let (tmp, dir) = fixture_data();
    let store = InstanceStore::new(&dir);
    let scan = store.scan().unwrap();
    let vanilla = scan.instances.iter().find(|i| i.id == "Vanilla").unwrap();
    store.save(vanilla).unwrap();

    let saved = fs::read_to_string(vanilla.path.join("instance.json")).unwrap();
    let expected = fixture_text("instances/Vanilla/instance.json")
        .replace(FIXTURE_ROOT, &tmp.path().to_string_lossy());
    assert_eq!(saved, expected);
}

#[test]
fn saved_instance_keeps_go_keys() {
    let (_tmp, dir) = fixture_data();
    let store = InstanceStore::new(&dir);
    for inst in store.scan().unwrap().instances {
        let rel = format!("instances/{}/instance.json", inst.id);
        store.save(&inst).unwrap();
        let saved = fs::read_to_string(inst.path.join("instance.json")).unwrap();
        let mut go_keys = top_level_keys(&fixture_text(&rel));
        if !go_keys.iter().any(|k| k == "createdAt") {
            // Go's `omitempty` never omits a time.Time: it always wrote one.
            let at = go_keys.iter().position(|k| k == "lastPlayed").unwrap() + 1;
            go_keys.insert(at, "createdAt".to_owned());
        }
        assert_eq!(top_level_keys(&saved), go_keys, "{rel}");
        let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        if inst.jvm_args.is_empty() {
            assert!(
                json["jvmArgs"].is_null(),
                "{rel}: Go wrote null for no args"
            );
        }
    }
}

/// Go saved the Java it picked into every instance it launched, here one of
/// its own runtimes. That's no choice of the user's: it's set aside, and the
/// instance forgets it.
#[tokio::test]
async fn go_java_pins_are_set_aside_and_forgotten() {
    use mctui_core::java::{Choice, Needs, Runtimes, Staleness};
    use mctui_core::launch::InstanceChanges;
    use mctui_core::mojang::rules::{Arch, Os};

    let (tmp, dir) = fixture_data();
    let store = InstanceStore::new(&dir);
    let scan = store.scan().unwrap();
    let mut fabric = scan
        .instances
        .into_iter()
        .find(|i| i.id == "1.21.6 Fabric")
        .unwrap();
    let pinned = Path::new("/home/steve/.config/mctui/java/21/bin/java");
    assert_eq!(fabric.java_path.as_deref(), Some(pinned));

    // Go's runtimes folder, whether or not it's mctui's too (it isn't in
    // portable mode). No Java is installed.
    let go_java = PathBuf::from("/home/steve/.config/mctui/java");
    let setups = [
        (go_java.clone(), None),
        (tmp.path().join("java"), Some(go_java)),
    ];
    let config = Config::load(&dir).value;
    let mut stale = None;
    for (managed, go_managed) in setups {
        let runtimes = Runtimes {
            managed,
            go_managed,
            java_home: None,
            path: Vec::new(),
            roots: Vec::new(),
            adoptium_url: String::new(),
            os: Os::Linux,
            arch: Arch::X86_64,
        };
        let choice = Choice::of(&fabric, &config);
        let pick = runtimes.find(&Needs::new(21), choice).await.unwrap();
        assert_eq!(pick.runtime, None, "nothing installed: install one");
        let pin = pick.stale.unwrap();
        assert_eq!((pin.path.as_path(), pin.why), (pinned, Staleness::Managed));
        stale = Some(pin);
    }

    // The launch hands it back, and the save leaves `javaPath` empty, as Go
    // wrote an unset one.
    let changes = InstanceChanges {
        stale_java: stale,
        ..InstanceChanges::default()
    };
    changes.apply(&mut fabric);
    assert_eq!(fabric.java_path, None);
    store.save(&fabric).unwrap();
    let saved = fs::read_to_string(fabric.path.join("instance.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(json["javaPath"], "");
}

#[test]
fn go_config_loads_and_round_trips() {
    let (tmp, dir) = fixture_data();
    let loaded = Config::load(&dir);
    assert!(loaded.notice.is_none());
    let config = loaded.value;
    assert_eq!(config.theme, "catppuccin");
    assert_eq!(config.java_path, None);
    assert_eq!(config.jvm_args, ["-Xmx4G", "-Xms512M"]);
    assert_eq!(config.launch_log_verbosity, LogVerbosity::Warn);

    config.save(&dir).unwrap();
    let saved = fs::read_to_string(dir.config_file()).unwrap();
    let expected = fixture_text("config.json").replace(FIXTURE_ROOT, &tmp.path().to_string_lossy());
    assert_eq!(saved, expected);
    assert_eq!(Config::load(&dir).value, config);
}

#[test]
fn go_config_from_another_data_dir_says_so() {
    let (_tmp, dir) = fixture_data();
    fs::write(dir.config_file(), fixture_text("config.json")).unwrap();
    let loaded = Config::load(&dir);
    let notice = loaded.notice.unwrap();
    assert!(
        notice.starts_with(&format!(
            "config.json points to {FIXTURE_ROOT}; mctui now uses "
        )),
        "{notice}"
    );
    assert_eq!(loaded.value.theme, "catppuccin", "settings still load");
}

#[test]
fn malformed_config_is_quarantined() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = DataDir::new(tmp.path());
    fs::write(dir.config_file(), r#"{"theme": "nord","#).unwrap();

    let loaded = Config::load(&dir);
    assert_eq!(loaded.value, Config::default());
    let notice = loaded.notice.unwrap();
    assert!(
        notice.starts_with(
            "config.json was damaged — started with default settings. The old file was kept as config.json.bad-"
        ),
        "{notice}"
    );
    assert!(!dir.config_file().exists());
    let bad: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains("config.json.bad-"))
        .collect();
    assert_eq!(bad.len(), 1);
    assert_eq!(fs::read_to_string(&bad[0]).unwrap(), r#"{"theme": "nord","#);
}

#[cfg(unix)]
#[test]
fn unreadable_config_is_left_alone() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let dir = DataDir::new(tmp.path());
    fs::write(dir.config_file(), "{}").unwrap();
    fs::set_permissions(dir.config_file(), fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(dir.config_file()).is_ok() {
        return; // running as root
    }
    let loaded = Config::load(&dir);
    assert_eq!(loaded.value, Config::default());
    let notice = loaded.notice.unwrap();
    assert!(
        notice.starts_with("Couldn't read config.json ("),
        "{notice}"
    );
    assert!(dir.config_file().exists());
}

#[test]
fn go_accounts_load_and_round_trip() {
    let (_tmp, dir) = fixture_data();
    let loaded = Accounts::load(&dir);
    assert!(loaded.notice.is_none());
    let accounts = loaded.value;

    let debug = format!("{accounts:?}");
    assert!(!debug.contains("test-access-token") && !debug.contains("test-refresh-token"));

    let [steve, alex] = accounts.accounts.as_slice() else {
        panic!("want two accounts, got {}", accounts.accounts.len());
    };
    assert_eq!(accounts.active().map(|a| a.name.as_str()), Some("Steve"));
    assert_eq!(steve.kind, AccountKind::Msa);
    assert_eq!(steve.access_token.expose(), "test-access-token");
    assert_eq!(
        steve.msa_refresh_token.as_ref().map(Secret::expose),
        Some("test-refresh-token")
    );
    assert_eq!(
        steve.expires_at,
        Some("2026-09-28T13:30:55.285468Z".parse().unwrap())
    );
    assert_eq!(alex.kind, AccountKind::Offline);
    assert_eq!(alex.expires_at, None);
    assert_eq!(alex.msa_refresh_token, None);
    assert!(!alex.is_expired(jiff::Timestamp::now()));

    accounts.save(&dir).unwrap();
    let saved = fs::read_to_string(dir.accounts_file()).unwrap();
    assert_eq!(top_level_keys(&saved), ["accounts", "activeId"]);
    assert_eq!(
        saved.matches("msaRefreshToken").count(),
        1,
        "omitted when empty"
    );
    let reloaded = Accounts::load(&dir).value;
    assert_eq!(reloaded.accounts, accounts.accounts);
    assert_eq!(reloaded.active_id, accounts.active_id);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(dir.accounts_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

/// Loads the real data dir (read-only) and checks nothing needs recovering.
/// Prints counts only, never token values.
#[test]
#[ignore = "reads the real ~/.local/share/mctui"]
fn real_data_loads_cleanly() {
    let home = std::env::var_os("HOME").expect("HOME is set");
    let dir = DataDir::new(Path::new(&home).join(".local/share/mctui"));
    assert!(
        dir.root().is_dir(),
        "no data dir at {}",
        dir.root().display()
    );

    // Loading a malformed file would quarantine (rename) it; refuse to run
    // rather than touch real data.
    for file in [dir.config_file(), dir.accounts_file()] {
        if let Ok(data) = fs::read(&file) {
            assert!(
                serde_json::from_slice::<serde_json::Value>(&data).is_ok(),
                "{} isn't valid JSON; not loading it",
                file.display()
            );
        }
    }

    let config = Config::load(&dir);
    let scan = InstanceStore::new(&dir).scan().unwrap();
    let accounts = Accounts::load(&dir);
    println!(
        "config: theme {:?}, notice: {}; instances: {}, problems: {}; accounts: {}, active: {}, notice: {}",
        config.value.theme,
        config.notice.is_some(),
        scan.instances.len(),
        scan.problems.len(),
        accounts.value.accounts.len(),
        accounts.value.active().is_some(),
        accounts.notice.is_some(),
    );
    assert!(config.notice.is_none(), "{:?}", config.notice);
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    assert!(accounts.notice.is_none(), "{:?}", accounts.notice);
}
