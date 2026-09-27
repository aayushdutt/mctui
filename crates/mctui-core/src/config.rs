//! `config.json`: app-wide settings.

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Deserializer, Serialize};

use crate::paths::DataDir;
use crate::{Loaded, fsutil};

/// The Microsoft app registration used for sign-in unless overridden.
pub const DEFAULT_MSA_CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";

/// JVM arguments used when the config doesn't set any.
pub const DEFAULT_JVM_ARGS: [&str; 2] = ["-Xmx2G", "-Xms512M"];

const DEFAULT_THEME: &str = "auto";

/// How much game output the launch view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogVerbosity {
    /// Errors only.
    #[default]
    Error,
    /// Errors and warnings.
    Warn,
    /// Everything.
    All,
}

/// Lenient, like Go's `ParseLaunchLogVerbosity`: case-insensitive, accepts a
/// few synonyms, and anything unknown means [`LogVerbosity::Error`].
impl<'de> Deserialize<'de> for LogVerbosity {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "warn" | "warning" | "warnings" => Self::Warn,
            "all" | "verbose" | "debug" => Self::All,
            _ => Self::Error,
        })
    }
}

/// App-wide settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The Java override from Settings; `None` finds or downloads one.
    pub java_path: Option<PathBuf>,
    pub jvm_args: Vec<String>,
    pub theme: String,
    pub show_snapshots: bool,
    pub msa_client_id: String,
    pub launch_log_verbosity: LogVerbosity,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            java_path: None,
            jvm_args: DEFAULT_JVM_ARGS.map(String::from).to_vec(),
            theme: DEFAULT_THEME.to_owned(),
            show_snapshots: false,
            msa_client_id: DEFAULT_MSA_CLIENT_ID.to_owned(),
            launch_log_verbosity: LogVerbosity::Error,
        }
    }
}

impl Config {
    /// A missing file gives defaults, silently. A malformed one is quarantined
    /// and defaults are used, with a notice. One that can't be read at all
    /// (permissions, I/O) gives defaults and a notice, and is left alone.
    pub fn load(dir: &DataDir) -> Loaded<Config> {
        const CONSEQUENCE: &str = "started with default settings";
        let path = dir.config_file();
        let data = match crate::read_optional(&path) {
            Ok(Some(data)) => data,
            Ok(None) => return Loaded::clean(Config::default()),
            Err(e) => {
                return Loaded {
                    value: Config::default(),
                    notice: Some(crate::unreadable_notice(&path, &e, CONSEQUENCE)),
                };
            }
        };
        match serde_json::from_slice::<ConfigFile>(&data) {
            Ok(file) => {
                let moved = moved_dirs_notice(&file, dir);
                let mut loaded = Loaded::clean(Config::from(file));
                if let Some(notice) = moved {
                    loaded.add_notice(notice);
                }
                loaded
            }
            Err(e) => Loaded {
                value: Config::default(),
                notice: Some(crate::quarantine_malformed(&path, &e, CONSEQUENCE).0),
            },
        }
    }

    /// Writes `config.json` atomically (mode 0644). What mctui doesn't use
    /// is kept as the file has it: the Go app's folder paths, and keys this
    /// version doesn't know.
    pub fn save(&self, dir: &DataDir) -> io::Result<()> {
        fs::create_dir_all(dir.root())?;
        let path = dir.config_file();
        let on_disk = crate::read_optional(&path)
            .ok()
            .flatten()
            .and_then(|data| serde_json::from_slice::<ConfigFile>(&data).ok());
        let data = serde_json::to_vec_pretty(&ConfigFile::new(self, dir, on_disk))?;
        fsutil::write_atomic(&path, &data, 0o644)
    }
}

/// Says so when `config.json` names folders other than the ones in use. The
/// Go app took its data dir from there, and its instances, assets, and
/// libraries folders; mctui takes them all from the data dir in use.
fn moved_dirs_notice(file: &ConfigFile, dir: &DataDir) -> Option<String> {
    let root = dir.root();
    let configured_root = Some(Path::new(&file.data_dir)).filter(|p| !p.as_os_str().is_empty());
    let mut notices = Vec::new();
    if let Some(configured) = configured_root
        && !same_dir(configured, root)
    {
        notices.push(format!(
            "config.json points to {}; mctui now uses {}.",
            configured.display(),
            root.display()
        ));
    }
    for (key, configured, name) in [
        ("instancesDir", &file.instances_dir, "instances"),
        ("assetsDir", &file.assets_dir, "assets"),
        ("librariesDir", &file.libraries_dir, "libraries"),
    ] {
        let configured = Path::new(configured);
        let in_use = root.join(name);
        // Go put each in its data dir unless told otherwise: one that's
        // there is covered by the data dir's notice.
        let beside_data_dir = configured_root.is_some_and(|d| same_dir(configured, &d.join(name)));
        if configured.as_os_str().is_empty() || same_dir(configured, &in_use) || beside_data_dir {
            continue;
        }
        notices.push(format!(
            "config.json sets {key} to {}; mctui uses {}.",
            configured.display(),
            in_use.display()
        ));
    }
    if notices.is_empty() {
        return None;
    }
    notices.push("Set MCTUI_DATA_DIR to use another folder.".to_owned());
    Some(notices.join(" "))
}

/// Whether two paths name the same folder: equal, or the same existing
/// folder through symlinks and `..`.
fn same_dir(a: &Path, b: &Path) -> bool {
    a == b || matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

/// `config.json` as Go wrote it, keys in Go's order.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ConfigFile {
    // Go loaded these over its defaults and used them: `dataDir` for the
    // config, accounts, and instances, and the others for their own folders
    // (instances aside, which Go kept in the data dir anyway). mctui takes
    // every path from the resolved [`DataDir`] instead, so these are read
    // only to warn when they point elsewhere. Saving keeps what the file
    // has, so a Go build reading it still finds its data, and writes the
    // resolved folders where it has none.
    #[serde(deserialize_with = "string_or_empty")]
    data_dir: String,
    #[serde(deserialize_with = "string_or_empty")]
    instances_dir: String,
    #[serde(deserialize_with = "string_or_empty")]
    assets_dir: String,
    #[serde(deserialize_with = "string_or_empty")]
    libraries_dir: String,
    java_path: String,
    jvm_args: Option<Vec<String>>,
    theme: String,
    show_snapshots: bool,
    #[serde(rename = "msaClientID")]
    msa_client_id: String,
    launch_log_verbosity: LogVerbosity,
    /// Keys this version doesn't know (a newer one's, say), kept as they are.
    #[serde(flatten)]
    other: serde_json::Map<String, serde_json::Value>,
}

/// A string, or `""` for any other value: a key we only warn about must not
/// cost the user their settings.
fn string_or_empty<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => s,
        _ => String::new(),
    })
}

impl ConfigFile {
    /// The file for `config`, keeping from `on_disk` what mctui doesn't use.
    fn new(config: &Config, dir: &DataDir, on_disk: Option<ConfigFile>) -> Self {
        let path = |p: &Path| p.to_string_lossy().into_owned();
        let kept = on_disk.unwrap_or_default();
        let or_in_use = |kept: String, in_use: &Path| {
            if kept.is_empty() { path(in_use) } else { kept }
        };
        Self {
            data_dir: or_in_use(kept.data_dir, dir.root()),
            instances_dir: or_in_use(kept.instances_dir, &dir.instances_dir()),
            assets_dir: or_in_use(kept.assets_dir, &dir.assets_dir()),
            libraries_dir: or_in_use(kept.libraries_dir, &dir.libraries_dir()),
            java_path: config.java_path.as_deref().map(path).unwrap_or_default(),
            jvm_args: Some(config.jvm_args.clone()),
            theme: config.theme.clone(),
            show_snapshots: config.show_snapshots,
            msa_client_id: config.msa_client_id.clone(),
            launch_log_verbosity: config.launch_log_verbosity,
            other: kept.other,
        }
    }
}

impl From<ConfigFile> for Config {
    fn from(file: ConfigFile) -> Self {
        let defaults = Config::default();
        let or_default = |s: String, default: String| if s.is_empty() { default } else { s };
        Self {
            java_path: crate::optional_path(file.java_path),
            jvm_args: file.jvm_args.unwrap_or(defaults.jvm_args),
            theme: or_default(file.theme, defaults.theme),
            show_snapshots: file.show_snapshots,
            msa_client_id: or_default(file.msa_client_id, defaults.msa_client_id),
            launch_log_verbosity: file.launch_log_verbosity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_str(json: &str) -> Config {
        serde_json::from_str::<ConfigFile>(json).unwrap().into()
    }

    #[test]
    fn empty_object_is_defaults() {
        assert_eq!(load_str("{}"), Config::default());
        assert_eq!(
            load_str(r#"{"jvmArgs":null,"theme":"","msaClientID":"","launchLogVerbosity":""}"#),
            Config::default()
        );
    }

    #[test]
    fn explicit_empty_jvm_args_stay_empty() {
        assert!(load_str(r#"{"jvmArgs":[]}"#).jvm_args.is_empty());
    }

    #[test]
    fn verbosity_is_lenient() {
        for (raw, want) in [
            ("warn", LogVerbosity::Warn),
            (" Warnings ", LogVerbosity::Warn),
            ("ALL", LogVerbosity::All),
            ("debug", LogVerbosity::All),
            ("error", LogVerbosity::Error),
            ("chatty", LogVerbosity::Error),
        ] {
            let json = format!(r#"{{"launchLogVerbosity":"{raw}"}}"#);
            assert_eq!(load_str(&json).launch_log_verbosity, want, "{raw}");
        }
    }

    #[test]
    fn data_dir_elsewhere_is_ignored_with_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        fs::write(
            data.config_file(),
            r#"{"dataDir":"/elsewhere","instancesDir":7,"theme":"nord"}"#,
        )
        .unwrap();
        let loaded = Config::load(&data);
        assert_eq!(loaded.value.theme, "nord");
        assert_eq!(
            loaded.notice.as_deref(),
            Some(
                format!(
                    "config.json points to /elsewhere; mctui now uses {}. Set MCTUI_DATA_DIR to use another folder.",
                    dir.path().display()
                )
                .as_str()
            )
        );

        // ...and kept on save, so a Go build reading the file still finds its
        // data there; a folder it gave as something else gets the real one.
        loaded.value.save(&data).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(data.config_file()).unwrap()).unwrap();
        assert_eq!(saved["dataDir"], "/elsewhere");
        let instances = dir.path().join("instances");
        assert_eq!(saved["instancesDir"], instances.to_string_lossy().as_ref());
        assert_eq!(Config::load(&data).notice, loaded.notice);
    }

    #[test]
    fn go_folder_paths_and_unknown_keys_are_kept_and_warned_about() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        let root = dir.path().to_string_lossy().into_owned();
        let json = serde_json::json!({
            "dataDir": root,
            "instancesDir": format!("{root}/instances"),
            "assetsDir": "/Volumes/Games/assets",
            "librariesDir": "/Volumes/Games/libraries",
            "theme": "nord",
            "someday": {"a": [1, 2]},
        });
        fs::write(data.config_file(), json.to_string()).unwrap();

        let loaded = Config::load(&data);
        assert_eq!(
            loaded.notice.as_deref(),
            Some(
                format!(
                    "config.json sets assetsDir to /Volumes/Games/assets; mctui uses {root}/assets. \
                     config.json sets librariesDir to /Volumes/Games/libraries; mctui uses \
                     {root}/libraries. Set MCTUI_DATA_DIR to use another folder."
                )
                .as_str()
            )
        );

        let mut config = loaded.value;
        config.theme = "dark".to_owned();
        config.save(&data).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(data.config_file()).unwrap()).unwrap();
        for key in [
            "dataDir",
            "instancesDir",
            "assetsDir",
            "librariesDir",
            "someday",
        ] {
            assert_eq!(saved[key], json[key], "{key}");
        }
        assert_eq!(saved["theme"], "dark");
        assert_eq!(Config::load(&data).notice, loaded.notice, "still true");
    }

    #[test]
    fn folders_in_the_configured_data_dir_share_its_notice() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        let json = serde_json::json!({
            "dataDir": "/elsewhere",
            "instancesDir": "/elsewhere/instances",
            "assetsDir": "/elsewhere/assets",
            "librariesDir": "/elsewhere/libraries",
        });
        fs::write(data.config_file(), json.to_string()).unwrap();
        let notice = Config::load(&data).notice.unwrap();
        assert_eq!(
            notice,
            format!(
                "config.json points to /elsewhere; mctui now uses {}. Set MCTUI_DATA_DIR to use another folder.",
                dir.path().display()
            )
        );
    }

    #[test]
    fn data_dir_matching_or_unset_is_quiet() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        let root = serde_json::to_string(&dir.path()).unwrap();
        let root_slash = serde_json::to_string(&format!("{}/", dir.path().display())).unwrap();
        let dotted = serde_json::to_string(&dir.path().join("instances/..")).unwrap();
        fs::create_dir(data.instances_dir()).unwrap();
        for value in [
            root,
            root_slash,
            dotted,
            r#""""#.into(),
            "null".into(),
            "7".into(),
        ] {
            fs::write(data.config_file(), format!(r#"{{"dataDir":{value}}}"#)).unwrap();
            let loaded = Config::load(&data);
            assert_eq!(loaded.notice, None, "dataDir {value}");
            assert_eq!(loaded.value, Config::default());
        }
    }

    #[cfg(unix)]
    #[test]
    fn data_dir_through_a_symlink_is_quiet() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let link = dir.path().join("link");
        fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(&root, &link).unwrap();
        let data = DataDir::new(&root);
        let json = serde_json::json!({ "dataDir": link }).to_string();
        fs::write(data.config_file(), json).unwrap();
        assert_eq!(Config::load(&data).notice, None);
    }

    #[test]
    fn missing_file_is_defaults_without_notice() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = Config::load(&DataDir::new(dir.path()));
        assert_eq!(loaded.value, Config::default());
        assert!(loaded.notice.is_none());
    }
}
