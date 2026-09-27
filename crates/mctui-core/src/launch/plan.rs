//! The launch plan: the command line and every file it needs, worked out
//! from the version's JSON without touching the disk or network.

use std::fmt;
use std::path::{Path, PathBuf};

use tokio::process::Command;

use super::game::{GameCommand, redact};
use super::identity::Identity;
use super::prepare::RESOURCES_URL;
use super::{Error, Result, user_args};
use crate::account::Secret;
use crate::config::Config;
use crate::fsutil;
use crate::instance::Instance;
use crate::mojang::rules::{Env, Os};
use crate::mojang::{Argument, Artifact, AssetIndexRef, LoggingConfig, VersionJson};
use crate::net::{Download, Hash};
use crate::paths::DataDir;

/// `${launcher_name}`: the launcher the game names in crash reports.
const LAUNCHER_NAME: &str = "mctui";

/// JVM arguments for versions from before `arguments.jvm` (1.12.2 and
/// older), as Mojang's launcher passes them. No `-XstartOnFirstThread`: the
/// LWJGL 2 those versions use hangs on macOS with it.
const LEGACY_JVM_ARGS: [&str; 5] = [
    "-Djava.library.path=${natives_directory}",
    "-Dminecraft.launcher.brand=${launcher_name}",
    "-Dminecraft.launcher.version=${launcher_version}",
    "-cp",
    "${classpath}",
];

/// What [`plan`] works from.
#[derive(Debug, Clone, Copy)]
pub struct PlanInputs<'a> {
    /// The version to launch: Mojang's JSON, or one with a loader merged in.
    pub version: &'a VersionJson,
    /// The instance to launch.
    pub instance: &'a Instance,
    /// The app's settings, for their JVM arguments.
    pub config: &'a Config,
    /// Where libraries and assets are kept.
    pub data: &'a DataDir,
    /// The `java` executable to run.
    pub java: &'a Path,
    /// Who plays.
    pub identity: &'a Identity,
    /// The platform to evaluate rules for: this computer's OS, and the
    /// architecture of the Java that runs the game.
    pub env: &'a Env,
}

/// Everything needed to start the game: the command line, and the files it
/// needs in place first. Its `Debug` output hides the access token.
#[derive(Clone, PartialEq, Eq)]
pub struct Plan {
    /// The game version's id, e.g. `1.21.8`.
    pub version: String,
    /// The `java` executable.
    pub java: PathBuf,
    /// Java's arguments, before the main class: the version's own, then the
    /// player's (Settings, then the instance's), then the logging setup.
    /// Later flags win in Java, so the player's override the version's, as
    /// in Mojang's launcher.
    pub jvm_args: Vec<String>,
    /// The class to run.
    pub main_class: String,
    /// The game's arguments, after the main class.
    pub game_args: Vec<String>,
    /// Where the game runs: the instance's `.minecraft`.
    pub working_dir: PathBuf,
    /// What Java loads classes from, in order: each library that applies
    /// here, once, then the game's jar.
    pub classpath: Vec<PathBuf>,
    /// Where native libraries are unpacked: `<instance>/natives`.
    pub natives_dir: PathBuf,
    /// Jars of native libraries to unpack there (versions before 1.19).
    pub natives: Vec<NativeJar>,
    /// The game's jar, libraries, natives jars, and logging setup to download.
    pub files: Vec<Download>,
    /// The version's assets, if it has any.
    pub assets: Option<Assets>,
    /// The instance's [`Instance::download_key`].
    pub download_key: String,
    /// The instance was fully downloaded under `download_key`, so its files
    /// only need checking that they're still there.
    pub downloaded: bool,
    /// The access token, to hide from `Debug`.
    secret: Option<Secret>,
}

/// A jar of native libraries to unpack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeJar {
    /// The jar, in the libraries folder.
    pub jar: PathBuf,
    /// Path prefixes inside it to leave out, e.g. `META-INF/`.
    pub exclude: Vec<String>,
}

/// A version's assets: sounds, languages, icons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assets {
    /// The asset index, which lists every object: `<assets>/indexes/<id>.json`.
    pub index: Download,
    /// Where objects are kept, by hash: `<assets>/objects`.
    pub objects_dir: PathBuf,
    /// Where objects are downloaded from: [`RESOURCES_URL`].
    pub objects_url: String,
    /// Where else the game expects them.
    pub layout: AssetLayout,
}

/// Where the game looks for its assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetLayout {
    /// In the objects folder, by hash: 1.7.3 on.
    Hashed,
    /// Also by name in this folder, `<assets>/virtual/legacy`: the `legacy`
    /// index of 1.6 to 1.7.2.
    Virtual(PathBuf),
    /// Also by name in this folder, the game's `resources`: the `pre-1.6`
    /// index of older versions.
    Resources(PathBuf),
}

impl AssetLayout {
    /// For the two asset indexes Mojang marks `virtual` and
    /// `map_to_resources`, which will never change.
    fn for_index(id: &str, assets_dir: &Path, game_dir: &Path) -> Self {
        match id {
            "legacy" => Self::Virtual(assets_dir.join("virtual").join(id)),
            "pre-1.6" => Self::Resources(game_dir.join("resources")),
            _ => Self::Hashed,
        }
    }

    /// The folder assets are copied into by name, if any.
    pub fn named_dir(&self) -> Option<&Path> {
        match self {
            Self::Hashed => None,
            Self::Virtual(dir) | Self::Resources(dir) => Some(dir),
        }
    }
}

/// Works out how to launch `inputs.instance` with `inputs.version`.
///
/// Libraries and native jars are chosen by the rules for `inputs.env`, and
/// each jar is on the classpath once (a version may list one twice, and the
/// Fabric merge keeps both). The client jar goes where the Go app kept it,
/// `<libraries>/com/mojang/minecraft/<id>/minecraft-<id>-client.jar`, so
/// existing downloads are reused.
pub fn plan(inputs: &PlanInputs<'_>) -> Result<Plan> {
    let &PlanInputs {
        version,
        instance,
        data,
        java,
        identity,
        env,
        ..
    } = inputs;
    let (libraries_dir, assets_dir) = (data.libraries_dir(), data.assets_dir());
    let game_dir = instance.game_dir();
    let natives_dir = instance.path.join("natives");

    let Jars {
        mut files,
        classpath,
        natives,
    } = jars(version, &libraries_dir, env)?;
    let logging = version
        .logging
        .client
        .as_ref()
        .map(|config| logging_file(config, &assets_dir))
        .transpose()?;
    let assets = version
        .asset_index
        .as_ref()
        .map(|index| assets(index, &assets_dir, &game_dir))
        .transpose()?;

    let separator = if env.os == Os::Windows { ";" } else { ":" };
    let classpath_arg = classpath
        .iter()
        .map(|path| utf8(path))
        .collect::<Result<Vec<_>>>()?
        .join(separator);
    let game_assets = match assets.as_ref().and_then(|a| a.layout.named_dir()) {
        Some(dir) => dir,
        None => &assets_dir,
    };
    let values = Placeholders {
        version,
        identity,
        auth_session: format!("token:{}:{}", identity.access_token(), identity.uuid()),
        assets_index: version
            .asset_index
            .as_ref()
            .map(|index| index.id.as_str())
            .or(version.assets.as_deref())
            .unwrap_or("legacy"),
        game_dir: utf8(&game_dir)?,
        assets_root: utf8(&assets_dir)?,
        game_assets: utf8(game_assets)?,
        natives_dir: utf8(&natives_dir)?,
        libraries_dir: utf8(&libraries_dir)?,
        classpath: &classpath_arg,
        separator,
    };

    let mut jvm_args = jvm_args(inputs, &values);
    if let Some((file, arg)) = logging {
        jvm_args.push(arg);
        files.push(file);
    }
    let legacy_game = version.minecraft_arguments.as_deref().unwrap_or_default();
    let game_args = values.fill_all(
        legacy_game
            .split_whitespace()
            .chain(templates(&version.arguments.game, env)),
    );

    let download_key = instance.download_key();
    Ok(Plan {
        version: version.id.clone(),
        java: java.to_owned(),
        jvm_args,
        main_class: version.main_class.clone(),
        game_args,
        working_dir: game_dir,
        classpath,
        natives_dir,
        natives,
        files,
        assets,
        downloaded: instance.is_fully_downloaded && instance.download_cache_key == download_key,
        download_key,
        secret: identity.secret().cloned(),
    })
}

impl Plan {
    /// Every argument after `java`: JVM arguments, main class, game arguments.
    pub fn args(&self) -> impl Iterator<Item = &str> {
        self.jvm_args
            .iter()
            .map(String::as_str)
            .chain([self.main_class.as_str()])
            .chain(self.game_args.iter().map(String::as_str))
    }

    /// The command that starts the game, in its working folder. Its `Debug`
    /// output hides the access token, and so does the game's output.
    pub fn command(&self) -> GameCommand {
        let mut command = Command::new(&self.java);
        command.args(self.args()).current_dir(&self.working_dir);
        GameCommand::new(command, self.secret.clone())
    }

    fn redacted(&self, args: &[String]) -> Vec<String> {
        args.iter()
            .map(|arg| redact(arg, self.secret.as_ref()).into_owned())
            .collect()
    }
}

impl fmt::Debug for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Plan")
            .field("version", &self.version)
            .field("java", &self.java)
            .field("jvm_args", &self.redacted(&self.jvm_args))
            .field("main_class", &self.main_class)
            .field("game_args", &self.redacted(&self.game_args))
            .field("working_dir", &self.working_dir)
            .field("classpath", &self.classpath)
            .field("natives_dir", &self.natives_dir)
            .field("natives", &self.natives)
            .field("files", &self.files)
            .field("assets", &self.assets)
            .field("download_key", &self.download_key)
            .field("downloaded", &self.downloaded)
            .finish_non_exhaustive()
    }
}

/// The jars a version uses: the files to download, the classpath, and the
/// natives to unpack.
struct Jars {
    files: Vec<Download>,
    classpath: Vec<PathBuf>,
    natives: Vec<NativeJar>,
}

/// The libraries that apply in `env`, then the client jar.
fn jars(version: &VersionJson, libraries_dir: &Path, env: &Env) -> Result<Jars> {
    let mut jars = Jars {
        files: Vec::new(),
        classpath: Vec::new(),
        natives: Vec::new(),
    };
    for library in version.libraries.iter().filter(|l| l.applies(env)) {
        let artifact = library.artifact().map_err(|source| Error::BadLibrary {
            version: version.id.clone(),
            source,
        })?;
        if let Some(artifact) = artifact {
            let file = library_file(libraries_dir, &artifact)?;
            push_new(&mut jars.classpath, &file.dest);
            jars.add(file);
        }
        if let Some(artifact) = library.native_artifact(env) {
            let file = library_file(libraries_dir, artifact)?;
            if !jars.natives.iter().any(|n| n.jar == file.dest) {
                jars.natives.push(NativeJar {
                    jar: file.dest.clone(),
                    exclude: library.extract.exclude.clone(),
                });
            }
            jars.add(file);
        }
    }
    let client = client_jar(version, libraries_dir)?;
    push_new(&mut jars.classpath, &client.dest);
    jars.add(client);
    Ok(jars)
}

impl Jars {
    /// Adds `file` to download, unless it's there already: versions list
    /// some jars more than once (1.16.5 has LWJGL's three times).
    fn add(&mut self, file: Download) {
        if !self.files.iter().any(|f| f.dest == file.dest) {
            self.files.push(file);
        }
    }
}

/// Java's arguments, but for the logging setup: the version's own, then
/// the player's.
fn jvm_args(inputs: &PlanInputs<'_>, values: &Placeholders<'_>) -> Vec<String> {
    let &PlanInputs {
        version,
        instance,
        config,
        env,
        ..
    } = inputs;
    // Before 1.13 there were no JVM arguments in the JSON; a loader may have
    // added some, but they don't include the classpath.
    let legacy = version.minecraft_arguments.is_some() || version.arguments.jvm.is_empty();
    let legacy_jvm: &[&str] = if legacy { &LEGACY_JVM_ARGS } else { &[] };
    let mut args = values.fill_all(
        legacy_jvm
            .iter()
            .copied()
            .chain(templates(&version.arguments.jvm, env)),
    );
    let user = user_args::merge(&config.jvm_args, &instance.jvm_args);
    if user.is_empty() {
        // Mojang's launcher does this for a profile without its own (26.1 on).
        args.extend(values.fill_all(templates(&version.arguments.default_user_jvm, env)));
    } else {
        args.extend(user);
    }
    args
}

/// The templates in `args` that apply in `env`.
fn templates<'a>(args: &'a [Argument], env: &Env) -> impl Iterator<Item = &'a str> {
    args.iter()
        .flat_map(|arg| arg.values_for(env))
        .map(String::as_str)
}

/// What each `${…}` stands for. Resolution, demo, and quick play
/// placeholders have none: those features are off, so their arguments are
/// never used.
struct Placeholders<'a> {
    version: &'a VersionJson,
    identity: &'a Identity,
    /// The pre-1.6 session id, `token:<access token>:<uuid>`.
    auth_session: String,
    assets_index: &'a str,
    game_dir: &'a str,
    assets_root: &'a str,
    game_assets: &'a str,
    natives_dir: &'a str,
    libraries_dir: &'a str,
    classpath: &'a str,
    separator: &'a str,
}

impl Placeholders<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        let identity = self.identity;
        Some(match key {
            "auth_player_name" => identity.name(),
            "auth_uuid" => identity.uuid(),
            "auth_access_token" => identity.access_token(),
            "auth_session" => &self.auth_session,
            "auth_xuid" => identity.xuid(),
            "user_type" => identity.user_type(),
            // Mojang's launcher sends its own; empty reads as absent.
            "clientid" => "",
            "user_properties" => "{}",
            "version_name" => &self.version.id,
            "version_type" => self.version.kind.as_str(),
            "game_directory" => self.game_dir,
            "assets_root" => self.assets_root,
            "game_assets" => self.game_assets,
            "assets_index_name" => self.assets_index,
            "natives_directory" => self.natives_dir,
            "library_directory" => self.libraries_dir,
            "classpath" => self.classpath,
            "classpath_separator" => self.separator,
            "launcher_name" => LAUNCHER_NAME,
            "launcher_version" => env!("CARGO_PKG_VERSION"),
            _ => return None,
        })
    }

    /// `template` with each known placeholder replaced, in one pass: a
    /// value is never itself searched for placeholders (the Go app's was,
    /// in random order: L-14). Unknown ones are left as they are.
    fn fill(&self, template: &str) -> String {
        let mut filled = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find("${") {
            let Some(len) = rest[start + 2..].find('}') else {
                break;
            };
            let (before, placeholder) = (&rest[..start], &rest[start..start + len + 3]);
            let key = &placeholder[2..placeholder.len() - 1];
            filled.push_str(before);
            if let Some(value) = self.get(key) {
                filled.push_str(value);
            } else {
                tracing::debug!(key, "leaving an unknown placeholder as it is");
                filled.push_str(placeholder);
            }
            rest = &rest[start + len + 3..];
        }
        filled.push_str(rest);
        filled
    }

    fn fill_all<'t>(&self, templates: impl IntoIterator<Item = &'t str>) -> Vec<String> {
        templates.into_iter().map(|t| self.fill(t)).collect()
    }
}

/// The client jar, where the Go app kept it.
fn client_jar(version: &VersionJson, libraries_dir: &Path) -> Result<Download> {
    let downloads = version.downloads.as_ref();
    let client = &downloads
        .ok_or_else(|| Error::NoClient(version.id.clone()))?
        .client;
    let dir = libraries_dir.join("com").join("mojang").join("minecraft");
    let dir = file_in(&dir, &version.id)?;
    let dest = dir.join(format!("minecraft-{}-client.jar", version.id));
    Ok(Download {
        url: client.url.clone(),
        dest,
        hash: Some(Hash::Sha1(client.sha1.clone())),
        size: Some(client.size),
    })
}

fn library_file(libraries_dir: &Path, artifact: &Artifact) -> Result<Download> {
    let dest = join_relative(libraries_dir, &artifact.path)
        .ok_or_else(|| Error::UnsafePath(artifact.path.clone()))?;
    Ok(Download {
        url: artifact.url.clone(),
        dest,
        hash: artifact
            .sha1
            .clone()
            .filter(|sha1| !sha1.is_empty())
            .map(Hash::Sha1),
        size: artifact.size,
    })
}

/// The logging setup, at `<assets>/log_configs/<id>` as in Mojang's
/// launcher, and the JVM argument that points the game at it. Passing it
/// matters: it turns off the lookups the 2021 log4j exploit used (L-2).
fn logging_file(config: &LoggingConfig, assets_dir: &Path) -> Result<(Download, String)> {
    let dest = file_in(&assets_dir.join("log_configs"), &config.file.id)?;
    let arg = config.argument.replace("${path}", utf8(&dest)?);
    let file = Download {
        url: config.file.url.clone(),
        dest,
        hash: Some(Hash::Sha1(config.file.sha1.clone())),
        size: Some(config.file.size),
    };
    Ok((file, arg))
}

fn assets(index: &AssetIndexRef, assets_dir: &Path, game_dir: &Path) -> Result<Assets> {
    let dest = file_in(&assets_dir.join("indexes"), &format!("{}.json", index.id))?;
    Ok(Assets {
        index: Download {
            url: index.url.clone(),
            dest,
            hash: Some(Hash::Sha1(index.sha1.clone())),
            size: Some(index.size),
        },
        objects_dir: assets_dir.join("objects"),
        objects_url: RESOURCES_URL.to_owned(),
        layout: AssetLayout::for_index(&index.id, assets_dir, game_dir),
    })
}

fn push_new(paths: &mut Vec<PathBuf>, path: &Path) {
    if !paths.iter().any(|p| p == path) {
        paths.push(path.to_owned());
    }
}

/// `path` as UTF-8, which Java's arguments need.
fn utf8(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error::NonUtf8Path(path.to_owned()))
}

/// `base` joined with `rel`, a `/`-separated relative path from metadata.
/// `None` when `rel` could reach outside `base`: empty, absolute, with `..`,
/// a drive letter, or a backslash.
pub(super) fn join_relative(base: &Path, rel: &str) -> Option<PathBuf> {
    rel.split('/').all(fsutil::is_plain_name).then(|| {
        rel.split('/')
            .fold(base.to_owned(), |path, part| path.join(part))
    })
}

/// `dir` joined with `name`, a single file or folder name from metadata.
fn file_in(dir: &Path, name: &str) -> Result<PathBuf> {
    if fsutil::is_plain_name(name) {
        Ok(dir.join(name))
    } else {
        Err(Error::UnsafePath(name.to_owned()))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A plan with nothing to download, for an instance at `instance`.
    pub(in crate::launch) fn empty_plan(instance: &Path) -> Plan {
        Plan {
            version: "test".into(),
            java: "java".into(),
            jvm_args: Vec::new(),
            main_class: "Main".into(),
            game_args: Vec::new(),
            working_dir: instance.join(".minecraft"),
            classpath: Vec::new(),
            natives_dir: instance.join("natives"),
            natives: Vec::new(),
            files: Vec::new(),
            assets: None,
            download_key: String::new(),
            downloaded: false,
            secret: None,
        }
    }

    #[test]
    fn relative_paths_stay_inside() {
        let base = Path::new("/libs");
        assert_eq!(
            join_relative(base, "org/lwjgl/lwjgl.jar"),
            Some(base.join("org").join("lwjgl").join("lwjgl.jar"))
        );
        for bad in [
            "",
            "/etc/passwd",
            "../x.jar",
            "a/../../x",
            "a//b",
            "./a",
            "a\\..\\b",
            "C:x",
            "a/",
        ] {
            assert_eq!(join_relative(base, bad), None, "{bad:?}");
        }
        assert!(file_in(base, "1.21.8").is_ok());
        assert!(file_in(base, "a/b").is_err());
        assert!(file_in(base, "..").is_err());
    }
}
