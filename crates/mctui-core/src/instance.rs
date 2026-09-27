//! Instances: one folder per game install under `<data>/instances`, each
//! described by an `instance.json`.

use std::cmp::Reverse;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::fsutil;
use crate::paths::DataDir;

const INSTANCE_FILE: &str = "instance.json";
/// Taken by every mctui changing an instance's `instance.json`, beside it.
const LOCK_FILE: &str = ".instance.json.lock";

/// The mod loader an instance runs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Loader {
    #[default]
    Vanilla,
    Fabric,
    Forge,
    Quilt,
    NeoForge,
    /// A loader this version doesn't know, kept verbatim.
    Other(String),
}

impl Loader {
    /// The id stored in `instance.json`.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::Quilt => "quilt",
            Self::NeoForge => "neoforge",
            Self::Other(id) => id,
        }
    }

    /// A label for people: "Fabric", or an unknown loader's raw id.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::Quilt => "Quilt",
            Self::NeoForge => "NeoForge",
            Self::Other(id) => id,
        }
    }

    /// Whether the in-app Modrinth browser can install mods for this loader.
    pub fn supports_mod_browser(&self) -> bool {
        matches!(self, Self::Fabric)
    }
}

/// Parses a stored id: case-insensitive, `""` means vanilla, and anything
/// unknown is kept as [`Loader::Other`].
impl From<&str> for Loader {
    fn from(id: &str) -> Self {
        match id.trim().to_lowercase().as_str() {
            "" | "vanilla" => Self::Vanilla,
            "fabric" => Self::Fabric,
            "forge" => Self::Forge,
            "quilt" => Self::Quilt,
            "neoforge" => Self::NeoForge,
            _ => Self::Other(id.to_owned()),
        }
    }
}

impl Serialize for Loader {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Loader {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Go read `null` as "", which is vanilla.
        let id = Option::<String>::deserialize(d)?.unwrap_or_default();
        Ok(Self::from(id.as_str()))
    }
}

/// One game install.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Instance {
    /// The folder name under the instances dir.
    pub id: String,
    /// Display name, freeform; may differ from the folder name.
    pub name: String,
    /// Minecraft version, e.g. "1.21.4".
    pub version: String,
    pub loader: Loader,
    pub loader_version: String,
    /// The instance folder.
    pub path: PathBuf,
    /// Java for this instance; `None` uses the global setting.
    pub java_path: Option<PathBuf>,
    /// Extra JVM arguments.
    pub jvm_args: Vec<String>,
    pub last_played: Option<Timestamp>,
    pub created_at: Option<Timestamp>,
    pub play_time_secs: u64,
    /// Every game file is downloaded, so the instance can launch offline.
    pub is_fully_downloaded: bool,
    /// When the instance was last fully downloaded.
    pub cached_at: Option<Timestamp>,
    /// [`Instance::download_key`] as of the last full download; a mismatch
    /// means the version or loader changed since.
    pub download_cache_key: String,
    /// Install the starter Fabric mods on first launch (cleared once done).
    pub install_starter_fabric_mods: bool,
}

impl Instance {
    /// The game's working directory: `<path>/.minecraft`.
    pub fn game_dir(&self) -> PathBuf {
        self.path.join(".minecraft")
    }

    /// When the instance was last touched, for most-recent-first lists: the
    /// later of `last_played` and `created_at`.
    pub fn recency(&self) -> Option<Timestamp> {
        self.last_played.max(self.created_at)
    }

    /// Fingerprints the game version, loader, and loader version, so a change
    /// to any of them invalidates [`Instance::is_fully_downloaded`].
    pub fn download_key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.version,
            self.loader.as_str().trim().to_lowercase(),
            self.loader_version.trim()
        )
    }
}

/// Turns a display name into a folder name: path-invalid characters become
/// `-`, control characters are dropped, and surrounding spaces and trailing
/// dots (invalid on Windows) are trimmed. Case, spaces, and Unicode are kept.
/// May return `""` (e.g. for `"...."`).
pub fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|&c| c >= ' ')
        .map(|c| if r#"<>:"/\|?*"#.contains(c) { '-' } else { c })
        .collect();
    cleaned
        .trim()
        .trim_end_matches(['.', ' '])
        .trim()
        .to_owned()
}

/// What [`InstanceStore::create`] needs to make a new instance.
#[derive(Debug, Clone, Default)]
pub struct NewInstance {
    pub name: String,
    pub version: String,
    pub loader: Loader,
    pub loader_version: String,
    pub install_starter_fabric_mods: bool,
}

/// The result of reading the instances dir.
#[derive(Debug, Default)]
pub struct Scan {
    /// Most recently used first, then by name.
    pub instances: Vec<Instance>,
    /// Folders whose `instance.json` couldn't be read or parsed.
    pub problems: Vec<ScanProblem>,
}

/// An instance folder that couldn't be loaded.
#[derive(Debug)]
pub struct ScanProblem {
    pub dir: PathBuf,
    pub error: String,
}

/// Reads and writes instances under `<data>/instances`.
#[derive(Debug, Clone)]
pub struct InstanceStore {
    dir: PathBuf,
}

impl InstanceStore {
    pub fn new(dir: &DataDir) -> Self {
        Self {
            dir: dir.instances_dir(),
        }
    }

    /// Loads every instance. A missing instances dir is an empty scan, and
    /// folders without an `instance.json` are skipped.
    pub fn scan(&self) -> io::Result<Scan> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Scan::default()),
            Err(e) => return Err(e),
        };
        let mut scan = Scan::default();
        for entry in entries {
            let dir = entry?.path();
            if !dir.is_dir() {
                continue;
            }
            match read_instance(&dir) {
                Ok(Some(inst)) => scan.instances.push(inst),
                Ok(None) => {}
                Err(error) => scan.problems.push(ScanProblem { dir, error }),
            }
        }
        scan.instances
            .sort_by_cached_key(|i| (Reverse(i.recency()), i.name.to_lowercase(), i.id.clone()));
        scan.problems.sort_by(|a, b| a.dir.cmp(&b.dir));
        Ok(scan)
    }

    /// Creates the folder (named from the display name, with " (2)", " (3)", …
    /// on collisions) and saves a new instance into it.
    pub fn create(&self, draft: NewInstance) -> io::Result<Instance> {
        let (id, path) = self.claim_dir(&draft.name)?;
        let inst = Instance {
            id,
            name: draft.name,
            version: draft.version,
            loader: draft.loader,
            loader_version: draft.loader_version,
            path,
            java_path: None,
            jvm_args: Vec::new(),
            last_played: None,
            created_at: Some(Timestamp::now()),
            play_time_secs: 0,
            is_fully_downloaded: false,
            cached_at: None,
            download_cache_key: String::new(),
            install_starter_fabric_mods: draft.install_starter_fabric_mods,
        };
        if let Err(e) = self.save(&inst) {
            // Don't leave an empty folder that would claim the name.
            let _ = fs::remove_dir(&inst.path);
            return Err(e);
        }
        Ok(inst)
    }

    /// Writes `<path>/instance.json` atomically (mode 0644). Refuses
    /// ([`io::ErrorKind::InvalidInput`]) unless `inst.path` is directly
    /// inside the instances dir. What another mctui saved since `inst` was
    /// read is lost: prefer [`update`](Self::update) for changes.
    pub fn save(&self, inst: &Instance) -> io::Result<()> {
        self.check_folder(&inst.path, "save")?;
        let data = serde_json::to_vec_pretty(&InstanceFile::from(inst))?;
        fsutil::write_atomic(&inst.path.join(INSTANCE_FILE), &data, 0o644)
    }

    /// Changes instance `id` without losing what another mctui (or a second
    /// window) saved meanwhile: under a lock every mctui takes to change the
    /// instance, reads its `instance.json` again, applies `change`, and
    /// saves it. Returns the instance as saved. Fails with
    /// [`io::ErrorKind::NotFound`] once the instance is gone.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut Instance)) -> io::Result<Instance> {
        let dir = self.dir.join(id);
        self.check_folder(&dir, "update")?;
        let _lock = fsutil::FileLock::acquire(&dir.join(LOCK_FILE))?;
        let mut inst = match read_instance(&dir) {
            Ok(Some(inst)) => inst,
            Ok(None) => {
                let gone = format!("{} has no {INSTANCE_FILE}", dir.display());
                return Err(io::Error::new(io::ErrorKind::NotFound, gone));
            }
            Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        };
        change(&mut inst);
        self.save(&inst)?;
        Ok(inst)
    }

    /// Deletes the instance folder and everything in it. Refuses
    /// ([`io::ErrorKind::InvalidInput`]) unless `inst.path` is directly inside
    /// the instances dir.
    pub fn delete(&self, inst: &Instance) -> io::Result<()> {
        self.check_folder(&inst.path, "delete")?;
        match fs::remove_dir_all(&inst.path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    /// Refuses, for `doing` to `path`, anything but a folder directly inside
    /// the instances dir. (A name that could reach elsewhere, such as `..`,
    /// `a/b` or an absolute path, joins onto the dir as something else.)
    fn check_folder(&self, path: &Path, doing: &str) -> io::Result<()> {
        if path.parent() == Some(self.dir.as_path()) && path.file_name().is_some() {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing to {doing} {}: it isn't an instance folder in {}",
                path.display(),
                self.dir.display()
            ),
        ))
    }

    /// Creates a fresh folder for `name`, returning its id and path.
    fn claim_dir(&self, name: &str) -> io::Result<(String, PathBuf)> {
        fs::create_dir_all(&self.dir)?;
        let base = match sanitize_dir_name(name) {
            s if s.is_empty() => "instance".to_owned(),
            s => s,
        };
        let mut id = base.clone();
        let mut n = 1;
        loop {
            let path = self.dir.join(&id);
            match fs::create_dir(&path) {
                Ok(()) => return Ok((id, path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    n += 1;
                    id = format!("{base} ({n})");
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// Reads `<dir>/instance.json`: `Ok(None)` when there isn't one. The id and
/// path come from the folder, whatever the file says.
fn read_instance(dir: &Path) -> Result<Option<Instance>, String> {
    let data = match crate::read_optional(&dir.join(INSTANCE_FILE)) {
        Ok(Some(data)) => data,
        Ok(None) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let id = instance_id(dir)?;
    let file: InstanceFile = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    Ok(Some(file.into_instance(id.to_owned(), dir.to_owned())))
}

/// The folder name, which must be valid UTF-8: a lossy id could match another
/// folder's, and then deleting one might remove the other.
fn instance_id(dir: &Path) -> Result<&str, String> {
    dir.file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| "the folder name isn't valid UTF-8; rename the folder to load it".to_owned())
}

/// `instance.json` as Go wrote it, keys in Go's order.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct InstanceFile {
    // Go trusted these; the folder is the source of truth now. Still written
    // for Go builds reading the file.
    #[serde(skip_deserializing)]
    id: String,
    name: String,
    version: String,
    loader: Loader,
    loader_ver: String,
    #[serde(skip_deserializing)]
    path: String,
    java_path: String,
    jvm_args: Option<Vec<String>>,
    #[serde(with = "crate::gotime")]
    last_played: Option<Timestamp>,
    #[serde(with = "crate::gotime")]
    created_at: Option<Timestamp>,
    play_time: i64,
    is_fully_downloaded: bool,
    #[serde(with = "crate::gotime")]
    cached_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "String::is_empty")]
    download_cache_key: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    install_starter_fabric_mods: bool,
}

impl InstanceFile {
    fn into_instance(self, id: String, path: PathBuf) -> Instance {
        Instance {
            id,
            name: self.name,
            version: self.version,
            loader: self.loader,
            loader_version: self.loader_ver,
            path,
            java_path: crate::optional_path(self.java_path),
            jvm_args: self.jvm_args.unwrap_or_default(),
            last_played: self.last_played,
            created_at: self.created_at,
            play_time_secs: u64::try_from(self.play_time).unwrap_or(0),
            is_fully_downloaded: self.is_fully_downloaded,
            cached_at: self.cached_at,
            download_cache_key: self.download_cache_key,
            install_starter_fabric_mods: self.install_starter_fabric_mods,
        }
    }
}

impl From<&Instance> for InstanceFile {
    fn from(inst: &Instance) -> Self {
        Self {
            id: inst.id.clone(),
            name: inst.name.clone(),
            version: inst.version.clone(),
            loader: inst.loader.clone(),
            loader_ver: inst.loader_version.clone(),
            path: inst.path.to_string_lossy().into_owned(),
            java_path: inst
                .java_path
                .as_deref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            // Go wrote `null` for "no extra args".
            jvm_args: (!inst.jvm_args.is_empty()).then(|| inst.jvm_args.clone()),
            last_played: inst.last_played,
            created_at: inst.created_at,
            play_time: i64::try_from(inst.play_time_secs).unwrap_or(i64::MAX),
            is_fully_downloaded: inst.is_fully_downloaded,
            cached_at: inst.cached_at,
            download_cache_key: inst.download_cache_key.clone(),
            install_starter_fabric_mods: inst.install_starter_fabric_mods,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn store() -> (tempfile::TempDir, InstanceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(&DataDir::new(dir.path()));
        (dir, store)
    }

    fn draft(name: &str) -> NewInstance {
        NewInstance {
            name: name.to_owned(),
            version: "1.21.4".to_owned(),
            ..NewInstance::default()
        }
    }

    #[test]
    fn sanitizes_dir_names_like_go() {
        for (name, want) in [
            ("My Fabric Pack", "My Fabric Pack"),
            ("a/b:c", "a-b-c"),
            ("weird/name:1", "weird-name-1"),
            ("  spaced  ", "spaced"),
            ("trailing... ", "trailing"),
            ("café 🎮 ok", "café 🎮 ok"),
            ("tab\there", "tabhere"),
            ("....", ""),
            ("", ""),
        ] {
            assert_eq!(sanitize_dir_name(name), want, "{name:?}");
        }
    }

    #[test]
    fn loader_ids() {
        assert_eq!(Loader::from(""), Loader::Vanilla);
        assert_eq!(Loader::from("Vanilla"), Loader::Vanilla);
        assert_eq!(Loader::from(" FABRIC "), Loader::Fabric);
        assert_eq!(Loader::from("neoforge"), Loader::NeoForge);
        assert_eq!(Loader::from("Rift"), Loader::Other("Rift".to_owned()));
        assert_eq!(Loader::Other("Rift".to_owned()).display_name(), "Rift");
        assert_eq!(Loader::NeoForge.display_name(), "NeoForge");
        assert_eq!(Loader::Vanilla.as_str(), "vanilla");
        assert!(Loader::Fabric.supports_mod_browser());
        assert!(!Loader::Quilt.supports_mod_browser());
        let loader: Loader = serde_json::from_str("null").unwrap();
        assert_eq!(loader, Loader::Vanilla);
    }

    #[test]
    fn download_key_matches_go() {
        let (_dir, store) = store();
        let mut inst = store.create(draft("x")).unwrap();
        assert_eq!(inst.download_key(), "1.21.4|vanilla|");
        inst.loader = Loader::from(" Rift ");
        inst.loader_version = " 0.1 ".to_owned();
        assert_eq!(inst.download_key(), "1.21.4|rift|0.1");
    }

    #[test]
    fn recency_is_the_later_time() {
        let (_dir, store) = store();
        let mut inst = store.create(draft("x")).unwrap();
        inst.created_at = Some(ts("2026-01-01T00:00:00Z"));
        assert_eq!(inst.recency(), inst.created_at);
        inst.last_played = Some(ts("2026-02-01T00:00:00Z"));
        assert_eq!(inst.recency(), inst.last_played);
        inst.created_at = None;
        inst.last_played = None;
        assert_eq!(inst.recency(), None);
    }

    #[test]
    fn create_dedups_folder_names() {
        let (_dir, store) = store();
        let a = store.create(draft("My Pack")).unwrap();
        let b = store.create(draft("My Pack")).unwrap();
        let c = store.create(draft("My/Pack")).unwrap();
        let d = store.create(draft("....")).unwrap();
        assert_eq!(a.id, "My Pack");
        assert_eq!(b.id, "My Pack (2)");
        assert_eq!(b.name, "My Pack");
        assert_eq!(c.id, "My-Pack");
        assert_eq!(d.id, "instance");
        assert!(b.path.join(INSTANCE_FILE).is_file());
        assert!(a.created_at.is_some());

        let scan = store.scan().unwrap();
        assert_eq!(scan.instances.len(), 4);
        assert!(scan.problems.is_empty());
    }

    #[test]
    fn delete_only_removes_instance_folders() {
        let (dir, store) = store();
        let inst = store.create(draft("doomed")).unwrap();

        let refused = |path: &Path| {
            let err = store
                .delete(&Instance {
                    path: path.to_owned(),
                    ..inst.clone()
                })
                .unwrap_err();
            assert_eq!(
                err.kind(),
                io::ErrorKind::InvalidInput,
                "{}",
                path.display()
            );
        };
        refused(dir.path());
        refused(&dir.path().join("instances"));
        refused(&dir.path().join("instances").join(".."));
        refused(&dir.path().join("elsewhere"));
        refused(&inst.path.join(".minecraft"));
        refused(Path::new("/"));
        assert!(inst.path.is_dir());

        store.delete(&inst).unwrap();
        assert!(!inst.path.exists());
        // Already gone is fine.
        store.delete(&inst).unwrap();
    }

    #[test]
    fn save_only_writes_instance_folders() {
        let (dir, store) = store();
        let inst = store.create(draft("kept")).unwrap();
        for path in [
            dir.path().to_owned(),
            dir.path().join("elsewhere"),
            dir.path().join("instances"),
            dir.path().join("instances").join(".."),
            inst.path.join(".minecraft"),
        ] {
            let err = store
                .save(&Instance {
                    path: path.clone(),
                    ..inst.clone()
                })
                .unwrap_err();
            assert_eq!(
                err.kind(),
                io::ErrorKind::InvalidInput,
                "{}",
                path.display()
            );
            assert!(!path.join(INSTANCE_FILE).exists(), "{}", path.display());
        }
    }

    #[test]
    fn updates_keep_what_another_window_saved() {
        let (_dir, store) = store();
        let inst = store.create(draft("shared")).unwrap();
        // Another window, holding an older copy, records a session...
        let played = store.update(&inst.id, |i| i.play_time_secs += 60).unwrap();
        assert_eq!(played.play_time_secs, 60);
        // ...and this one, holding its own, renames the instance.
        let renamed = store
            .update(&inst.id, |i| i.name = "Renamed".into())
            .unwrap();
        assert_eq!(
            (renamed.name.as_str(), renamed.play_time_secs),
            ("Renamed", 60)
        );
        let scanned = store.scan().unwrap().instances.remove(0);
        assert_eq!(scanned, renamed);
    }

    #[test]
    fn concurrent_updates_are_all_kept() {
        let (_dir, store) = store();
        let inst = store.create(draft("busy")).unwrap();
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| store.update(&inst.id, |i| i.play_time_secs += 1).unwrap());
            }
        });
        assert_eq!(store.scan().unwrap().instances[0].play_time_secs, 8);
    }

    #[test]
    fn updating_a_missing_or_unsafe_instance_fails() {
        let (dir, store) = store();
        store.create(draft("here")).unwrap();
        let err = store.update("gone", |_| {}).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound, "{err}");
        assert!(!dir.path().join("instances/gone").exists());
        for id in ["", "..", "../here", "a/b"] {
            let err = store.update(id, |_| {}).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{id:?}");
        }
    }

    #[test]
    fn scan_of_missing_dir_is_empty() {
        let (_dir, store) = store();
        let scan = store.scan().unwrap();
        assert!(scan.instances.is_empty() && scan.problems.is_empty());
    }

    #[test]
    fn scan_sorts_recent_first_then_by_name_then_id() {
        let (_dir, store) = store();
        let add = |id: &str, name: &str, last_played: Option<&str>| {
            let mut inst = store.create(draft(id)).unwrap();
            inst.name = name.to_owned();
            inst.created_at = None;
            inst.last_played = last_played.map(ts);
            store.save(&inst).unwrap();
        };
        add("old", "Zed", Some("2026-01-01T00:00:00Z"));
        add("new", "Zed", Some("2026-02-01T00:00:00Z"));
        add("b-upper", "B", None);
        add("a-lower", "a", None);
        add("b-lower", "b", None);
        let ids: Vec<String> = store
            .scan()
            .unwrap()
            .instances
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(ids, ["new", "old", "a-lower", "b-lower", "b-upper"]);
    }

    #[test]
    fn negative_play_time_reads_as_zero() {
        let file: InstanceFile = serde_json::from_str(r#"{"playTime":-5}"#).unwrap();
        let inst = file.into_instance("x".to_owned(), PathBuf::from("/x"));
        assert_eq!(inst.play_time_secs, 0);
    }

    #[cfg(unix)]
    mod unix {
        use std::os::unix::ffi::OsStrExt;

        use super::*;

        /// `café` in Latin-1: not UTF-8.
        fn latin1_name() -> &'static OsStr {
            OsStr::from_bytes(b"caf\xe9")
        }

        #[test]
        fn non_utf8_folder_name_has_no_id() {
            let dir = Path::new("/data/instances");
            let err = instance_id(&dir.join(latin1_name())).unwrap_err();
            assert!(err.contains("isn't valid UTF-8"), "{err}");
            assert_eq!(instance_id(&dir.join("café")), Ok("café"));
        }

        #[test]
        fn scan_reports_non_utf8_folders() {
            let (_dir, store) = store();
            let good = store.create(draft("café")).unwrap();
            let bad = store.dir.join(latin1_name());
            if fs::create_dir(&bad).is_err() {
                return; // e.g. APFS, which only allows UTF-8 names
            }
            fs::copy(good.path.join(INSTANCE_FILE), bad.join(INSTANCE_FILE)).unwrap();

            let scan = store.scan().unwrap();
            let ids: Vec<&str> = scan.instances.iter().map(|i| i.id.as_str()).collect();
            assert_eq!(ids, ["café"]);
            let [problem] = scan.problems.as_slice() else {
                panic!("want one problem, got {:?}", scan.problems);
            };
            assert_eq!(problem.dir, bad);
            assert!(
                problem.error.contains("isn't valid UTF-8"),
                "{}",
                problem.error
            );
        }
    }
}
