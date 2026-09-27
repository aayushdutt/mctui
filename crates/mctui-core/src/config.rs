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
                let moved = moved_data_dir_notice(&file.data_dir, dir);
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

    /// Writes `config.json` atomically (mode 0644).
    pub fn save(&self, dir: &DataDir) -> io::Result<()> {
        fs::create_dir_all(dir.root())?;
        let data = serde_json::to_vec_pretty(&ConfigFile::new(self, dir))?;
        fsutil::write_atomic(&dir.config_file(), &data, 0o644)
    }
}

/// Says so when `config.json` names a data dir other than the one in use.
fn moved_data_dir_notice(configured: &str, dir: &DataDir) -> Option<String> {
    let (configured, root) = (Path::new(configured), dir.root());
    if configured.as_os_str().is_empty() || configured == root || same_dir(configured, root) {
        return None;
    }
    Some(format!(
        "config.json points to {}; mctui now uses {}. Set MCTUI_DATA_DIR to use another folder.",
        configured.display(),
        root.display()
    ))
}

/// Whether two paths name the same existing folder, through symlinks and `..`.
fn same_dir(a: &Path, b: &Path) -> bool {
    matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

/// `config.json` as Go wrote it, keys in Go's order.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ConfigFile {
    // Go loaded these over its defaults and used them: `dataDir` for the
    // config, accounts, and instances, the others for their own folders. mctui
    // takes every path from the resolved [`DataDir`] instead, so `dataDir` is
    // read only to warn when it points elsewhere and the rest are ignored. All
    // four are still written, from the resolved dir, so an older Go build
    // reading this file finds its data.
    #[serde(deserialize_with = "string_or_empty")]
    data_dir: String,
    #[serde(skip_deserializing)]
    instances_dir: String,
    #[serde(skip_deserializing)]
    assets_dir: String,
    #[serde(skip_deserializing)]
    libraries_dir: String,
    java_path: String,
    jvm_args: Option<Vec<String>>,
    theme: String,
    show_snapshots: bool,
    #[serde(rename = "msaClientID")]
    msa_client_id: String,
    launch_log_verbosity: LogVerbosity,
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
    fn new(config: &Config, dir: &DataDir) -> Self {
        let path = |p: &Path| p.to_string_lossy().into_owned();
        Self {
            data_dir: path(dir.root()),
            instances_dir: path(&dir.instances_dir()),
            assets_dir: path(&dir.assets_dir()),
            libraries_dir: path(&dir.libraries_dir()),
            java_path: config.java_path.as_deref().map(path).unwrap_or_default(),
            jvm_args: Some(config.jvm_args.clone()),
            theme: config.theme.clone(),
            show_snapshots: config.show_snapshots,
            msa_client_id: config.msa_client_id.clone(),
            launch_log_verbosity: config.launch_log_verbosity,
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
            loaded.notice.unwrap(),
            format!(
                "config.json points to /elsewhere; mctui now uses {}. Set MCTUI_DATA_DIR to use another folder.",
                dir.path().display()
            )
        );

        // ...and rewritten from the real data dir on save, which settles it.
        loaded.value.save(&data).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(data.config_file()).unwrap()).unwrap();
        assert_eq!(saved["dataDir"], dir.path().to_string_lossy().as_ref());
        assert!(Config::load(&data).notice.is_none());
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
