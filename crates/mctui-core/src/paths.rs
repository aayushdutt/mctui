//! Where mctui keeps its files.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::{env, fs};

/// Why the data dir couldn't be resolved.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("couldn't find your home directory")]
    NoHomeDir,
}

/// The root of mctui's data: config, accounts, instances, and game assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    root: PathBuf,
    /// The `data` folder beside the executable: portable mode keeps
    /// everything in it, managed Java too.
    portable: bool,
}

impl DataDir {
    /// Resolution order: `$MCTUI_DATA_DIR`, `<exe dir>/data` if it exists
    /// (portable mode), `$XDG_DATA_HOME/mctui`, `%APPDATA%/mctui` on Windows,
    /// then `~/.local/share/mctui` (Linux and macOS alike, as the Go app did).
    pub fn resolve() -> Result<DataDir, Error> {
        let exe = env::current_exe().ok();
        resolve_with(
            |key| env::var_os(key),
            exe.as_deref().and_then(Path::parent),
            dirs::home_dir(),
        )
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            portable: false,
        }
    }

    /// Portable mode's data dir, which also keeps managed Java: see
    /// [`java_runtimes_dir`].
    pub fn portable(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            portable: true,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether this is portable mode's `data` folder beside the executable.
    pub fn is_portable(&self) -> bool {
        self.portable
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// Creates the root and the instances, assets, and libraries dirs.
    pub fn create_dirs(&self) -> io::Result<()> {
        for dir in [
            self.instances_dir(),
            self.assets_dir(),
            self.libraries_dir(),
        ] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

fn resolve_with(
    var: impl Fn(&str) -> Option<OsString>,
    exe_dir: Option<&Path>,
    home: Option<PathBuf>,
) -> Result<DataDir, Error> {
    let var = |key| var(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = var("MCTUI_DATA_DIR") {
        return Ok(DataDir::new(dir));
    }
    if let Some(portable) = exe_dir.map(|d| d.join("data")).filter(|d| d.is_dir()) {
        return Ok(DataDir::portable(portable));
    }
    if let Some(xdg) = var("XDG_DATA_HOME") {
        return Ok(DataDir::new(xdg.join("mctui")));
    }
    if cfg!(windows)
        && let Some(appdata) = var("APPDATA")
    {
        return Ok(DataDir::new(appdata.join("mctui")));
    }
    let home = home.ok_or(Error::NoHomeDir)?;
    Ok(DataDir::new(
        home.join(".local").join("share").join("mctui"),
    ))
}

/// Managed Java runtimes for `data`: in portable mode `<data>/java`, else
/// [`go_java_runtimes_dir`], outside the data dir as in the Go app.
pub fn java_runtimes_dir(data: &DataDir) -> Option<PathBuf> {
    if data.is_portable() {
        Some(data.root().join("java"))
    } else {
        go_java_runtimes_dir()
    }
}

/// Where the Go app kept managed Java runtimes, portable or not:
/// `<config dir>/mctui/java` (e.g. `~/Library/Application Support/mctui/java`
/// on macOS).
pub fn go_java_runtimes_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mctui").join("java"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(vars: &[(&str, &str)], exe_dir: Option<&Path>) -> Result<DataDir, Error> {
        let vars: Vec<(String, OsString)> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect();
        resolve_with(
            |key| vars.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
            exe_dir,
            Some(PathBuf::from("/home/steve")),
        )
    }

    #[test]
    fn resolution_order() {
        let portable = tempfile::tempdir().unwrap();
        fs::create_dir(portable.path().join("data")).unwrap();
        let all = [("MCTUI_DATA_DIR", "/override"), ("XDG_DATA_HOME", "/xdg")];

        let dir = resolve(&all, Some(portable.path())).unwrap();
        assert_eq!(dir.root(), Path::new("/override"));
        assert!(!dir.is_portable(), "an explicit dir isn't portable");

        let dir = resolve(&all[1..], Some(portable.path())).unwrap();
        assert_eq!(dir.root(), portable.path().join("data"));
        assert!(dir.is_portable());

        let dir = resolve(&all[1..], Some(Path::new("/no/such/dir"))).unwrap();
        assert_eq!(dir.root(), Path::new("/xdg/mctui"));
        assert!(!dir.is_portable());

        // Empty variables are ignored, as Go did.
        let dir = resolve(&[("MCTUI_DATA_DIR", ""), ("XDG_DATA_HOME", "")], None).unwrap();
        assert_eq!(dir.root(), Path::new("/home/steve/.local/share/mctui"));
    }

    #[test]
    fn no_home_is_an_error() {
        let result = resolve_with(|_| None, None, None);
        assert!(matches!(result, Err(Error::NoHomeDir)));
    }

    #[test]
    fn file_layout() {
        let dir = DataDir::new("/data");
        assert_eq!(dir.config_file(), Path::new("/data/config.json"));
        assert_eq!(dir.accounts_file(), Path::new("/data/accounts.json"));
        assert_eq!(dir.instances_dir(), Path::new("/data/instances"));
        assert_eq!(dir.cache_dir(), Path::new("/data/cache"));
    }

    #[test]
    fn portable_mode_keeps_java_in_the_data_dir() {
        let portable = DataDir::portable("/usb/mctui/data");
        assert_eq!(
            java_runtimes_dir(&portable),
            Some(PathBuf::from("/usb/mctui/data/java"))
        );
        // Otherwise it's where the Go app kept it.
        let usual = DataDir::new("/data");
        assert_eq!(java_runtimes_dir(&usual), go_java_runtimes_dir());
    }
}
