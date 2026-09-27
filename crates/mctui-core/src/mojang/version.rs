//! A version JSON: everything needed to download and launch one version of
//! the game.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::Deserialize;

use super::manifest::VersionKind;
use super::maven::{Coordinate, InvalidCoordinate};
use super::rules::{self, Env, Rule};

/// Where a library without download info comes from, as in Mojang's
/// launcher.
const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

/// A version JSON from Mojang, or a mod loader profile that builds on one
/// (see `inherits_from`). A profile leaves out what it inherits, which is why
/// much here is optional.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    /// The version id, e.g. `1.21.8`; also `${version_name}`.
    pub id: String,
    /// Release, snapshot, and so on; also `${version_type}`.
    #[serde(rename = "type")]
    pub kind: VersionKind,
    /// The class to run, e.g. `net.minecraft.client.main.Main`.
    pub main_class: String,
    /// The version a loader profile builds on.
    pub inherits_from: Option<String>,
    /// Before 1.13: the game arguments as one string of `${…}` templates.
    pub minecraft_arguments: Option<String>,
    /// From 1.13: game and JVM arguments. Empty before.
    #[serde(default)]
    pub arguments: Arguments,
    /// Where the list of assets (sounds, languages) is.
    pub asset_index: Option<AssetIndexRef>,
    /// The asset index id, e.g. `1.16` (`legacy` and `pre-1.6` for old
    /// versions).
    pub assets: Option<String>,
    /// The client jar and friends.
    pub downloads: Option<Downloads>,
    /// The Java this version needs.
    pub java_version: Option<JavaVersion>,
    /// Everything for the classpath, and natives, for every platform.
    #[serde(default)]
    pub libraries: Vec<Library>,
    /// The log configuration to pass to the game.
    #[serde(default)]
    pub logging: Logging,
    /// When the version came out.
    pub release_time: Option<Timestamp>,
    /// 0 for versions from before Mojang's player safety features, which
    /// Mojang's launcher warns about.
    #[serde(default)]
    pub compliance_level: u32,
}

/// Argument templates, with `${…}` placeholders for the launcher to fill.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Arguments {
    /// Arguments for the game, after the main class.
    #[serde(default)]
    pub game: Vec<Argument>,
    /// Arguments for Java, before the main class.
    #[serde(default)]
    pub jvm: Vec<Argument>,
    /// JVM options Mojang's launcher suggests for a new profile, such as heap
    /// size and garbage collector (26.1 on).
    #[serde(default, rename = "default-user-jvm")]
    pub default_user_jvm: Vec<Argument>,
}

/// One entry of an argument list: values used when `rules` allow. A plain
/// string in the JSON is one value with no rules.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "RawArgument")]
pub struct Argument {
    /// When the values apply; always when empty.
    pub rules: Vec<Rule>,
    /// Templates such as `--username` and `${auth_player_name}`.
    pub values: Vec<String>,
}

impl Argument {
    /// The values to pass in `env`: all of them if the rules allow, else none.
    pub fn values_for(&self, env: &Env) -> &[String] {
        if rules::allowed(&self.rules, env) {
            &self.values
        } else {
            &[]
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawArgument {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Vec<Rule>,
        value: OneOrMany,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl From<RawArgument> for Argument {
    fn from(raw: RawArgument) -> Self {
        let (rules, values) = match raw {
            RawArgument::Plain(value) => (Vec::new(), vec![value]),
            RawArgument::Conditional { rules, value } => match value {
                OneOrMany::One(value) => (rules, vec![value]),
                OneOrMany::Many(values) => (rules, values),
            },
        };
        Self { rules, values }
    }
}

/// A file to download, as the manifest and version JSONs describe it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemoteFile {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// Where the version's asset index is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    /// The size of every asset it lists, together.
    pub total_size: u64,
    pub url: String,
}

/// The game's own files.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Downloads {
    /// The client jar.
    pub client: RemoteFile,
    /// Obfuscation mappings for the client (1.14.4 on).
    pub client_mappings: Option<RemoteFile>,
    /// The server jar.
    pub server: Option<RemoteFile>,
    /// Obfuscation mappings for the server (1.14.4 on).
    pub server_mappings: Option<RemoteFile>,
}

/// The Java a version needs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    /// Mojang's runtime name, e.g. `java-runtime-delta`.
    pub component: String,
    /// e.g. 21.
    pub major_version: u32,
}

/// Logging setup Mojang ships with the game.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Logging {
    /// The client's configuration; missing for the oldest versions.
    pub client: Option<LoggingConfig>,
}

/// A log4j configuration for the client. Passing it matters: for versions
/// with a vulnerable log4j, Mojang's configuration turns off the message
/// lookups that the 2021 exploit used.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LoggingConfig {
    /// The JVM argument, e.g. `-Dlog4j.configurationFile=${path}`.
    pub argument: String,
    /// The configuration file, for `${path}`.
    pub file: LoggingFile,
    /// e.g. `log4j2-xml`.
    #[serde(rename = "type")]
    pub kind: String,
}

/// The logging configuration file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LoggingFile {
    /// Its file name, e.g. `client-1.12.xml`.
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// A library: a jar for the classpath, and before 1.19 maybe native code to
/// extract.
///
/// Mojang describes the files in `downloads`. Loader profiles give a Maven
/// repository `url` (and maybe a checksum) instead, and the file's path
/// follows from `name`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Library {
    /// A Maven coordinate, e.g. `org.lwjgl:lwjgl:3.3.3:natives-linux`.
    pub name: String,
    /// Mojang's description of the files.
    pub downloads: Option<LibraryDownloads>,
    /// The Maven repository to fetch from when there's no `downloads`.
    pub url: Option<String>,
    /// SHA-1 of the jar, when a loader profile publishes one.
    pub sha1: Option<String>,
    /// Size of the jar, when a loader profile publishes it.
    pub size: Option<u64>,
    /// Where the library is used; everywhere when empty.
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Before 1.19: the classifier of the natives jar for each OS (`windows`,
    /// `osx`, `linux`), which may contain `${arch}`.
    #[serde(default)]
    pub natives: BTreeMap<String, String>,
    /// What to leave out when extracting natives.
    #[serde(default)]
    pub extract: Extract,
}

/// Mojang's download info for a library.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct LibraryDownloads {
    /// The main jar; missing for libraries that only ship natives.
    pub artifact: Option<Artifact>,
    /// Other jars by classifier, e.g. `natives-linux`.
    #[serde(default)]
    pub classifiers: BTreeMap<String, Artifact>,
}

/// A library file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Artifact {
    /// Where it goes under the libraries folder, with `/` separators.
    pub path: String,
    pub url: String,
    /// Always set by Mojang; loader profiles leave it out for some jars.
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// Paths to skip when extracting a natives jar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Extract {
    /// Path prefixes, e.g. `META-INF/`.
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Library {
    /// Whether the library is used in `env`.
    pub fn applies(&self, env: &Env) -> bool {
        rules::allowed(&self.rules, env)
    }

    /// The jar for the classpath: Mojang's `downloads.artifact`, or for a
    /// library without `downloads`, the Maven path of `name` under `url`
    /// (Mojang's library server when there's no `url` either). `None` when
    /// the library only ships natives.
    pub fn artifact(&self) -> Result<Option<Artifact>, InvalidCoordinate> {
        if let Some(downloads) = &self.downloads {
            return Ok(downloads.artifact.clone());
        }
        let path = Coordinate::parse(&self.name)?.path();
        let repo = self.url.as_deref().unwrap_or(MOJANG_LIBRARIES);
        Ok(Some(Artifact {
            url: format!("{}/{path}", repo.trim_end_matches('/')),
            path,
            sha1: self.sha1.clone(),
            size: self.size,
        }))
    }

    /// The natives jar to extract in `env`, for libraries that list
    /// `natives` (before 1.19). From 1.19, natives are plain libraries with
    /// a classifier in their name.
    pub fn native_artifact(&self, env: &Env) -> Option<&Artifact> {
        let classifier = self
            .natives
            .get(env.os.as_str())?
            .replace("${arch}", env.arch.bits());
        self.downloads.as_ref()?.classifiers.get(&classifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mojang::rules::{Arch, Os};

    fn env(os: Os, arch: Arch) -> Env {
        Env {
            os,
            arch,
            os_version: String::new(),
            features: Vec::new(),
        }
    }

    #[test]
    fn argument_shapes() {
        let args: Vec<Argument> = serde_json::from_str(
            r#"["--demo-less",
                {"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": ["-XstartOnFirstThread"]},
                {"rules": [{"action": "allow", "os": {"arch": "x86"}}], "value": "-Xss1M"},
                {"value": ["-Xms2G", "-Xmx4G"]}]"#,
        )
        .unwrap();
        let mac = env(Os::MacOs, Arch::Arm64);
        let win32 = env(Os::Windows, Arch::X86);
        let resolved = |env: &Env| -> Vec<&str> {
            args.iter()
                .flat_map(|a| a.values_for(env))
                .map(String::as_str)
                .collect()
        };
        assert_eq!(
            resolved(&mac),
            ["--demo-less", "-XstartOnFirstThread", "-Xms2G", "-Xmx4G"]
        );
        assert_eq!(
            resolved(&win32),
            ["--demo-less", "-Xss1M", "-Xms2G", "-Xmx4G"]
        );
    }

    #[test]
    fn maven_style_libraries_get_their_path_from_the_name() {
        let lib: Library = serde_json::from_str(
            r#"{"name": "org.ow2.asm:asm:9.10.1", "url": "https://maven.fabricmc.net/",
                "sha1": "ada2141c0cc52ee8f5c48cd5fa4ce0e794f22236", "size": 126151}"#,
        )
        .unwrap();
        assert_eq!(
            lib.artifact().unwrap(),
            Some(Artifact {
                path: "org/ow2/asm/asm/9.10.1/asm-9.10.1.jar".into(),
                url: "https://maven.fabricmc.net/org/ow2/asm/asm/9.10.1/asm-9.10.1.jar".into(),
                sha1: Some("ada2141c0cc52ee8f5c48cd5fa4ce0e794f22236".into()),
                size: Some(126_151),
            })
        );

        let bare: Library = serde_json::from_str(r#"{"name": "com.mojang:realms:1.3.5"}"#).unwrap();
        let artifact = bare.artifact().unwrap().unwrap();
        assert_eq!(
            artifact.url,
            "https://libraries.minecraft.net/com/mojang/realms/1.3.5/realms-1.3.5.jar"
        );
        assert_eq!((artifact.sha1, artifact.size), (None, None));

        let bad: Library =
            serde_json::from_str(r#"{"name": "nope", "url": "https://x/"}"#).unwrap();
        assert_eq!(bad.artifact(), Err(InvalidCoordinate("nope".into())));
    }

    #[test]
    fn natives_only_libraries_have_no_artifact() {
        let lib: Library = serde_json::from_str(
            r#"{"name": "tv.twitch:twitch-platform:6.5",
                "natives": {"osx": "natives-osx", "windows": "natives-windows-${arch}"},
                "downloads": {"classifiers": {
                    "natives-osx": {"path": "o.jar", "sha1": "a", "size": 1, "url": "https://x/o.jar"},
                    "natives-windows-32": {"path": "w32.jar", "sha1": "b", "size": 2, "url": "https://x/w32.jar"},
                    "natives-windows-64": {"path": "w64.jar", "sha1": "c", "size": 3, "url": "https://x/w64.jar"}}}}"#,
        )
        .unwrap();
        assert_eq!(lib.artifact(), Ok(None));
        let native = |os, arch| lib.native_artifact(&env(os, arch)).map(|a| a.path.as_str());
        assert_eq!(native(Os::Windows, Arch::X86), Some("w32.jar"));
        assert_eq!(native(Os::Windows, Arch::X86_64), Some("w64.jar"));
        assert_eq!(native(Os::MacOs, Arch::Arm64), Some("o.jar"));
        assert_eq!(native(Os::Linux, Arch::X86_64), None);
    }
}
