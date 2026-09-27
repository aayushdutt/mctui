//! Modrinth's API types, trimmed to what mctui uses, and the rule for picking
//! the version to install.

use jiff::Timestamp;
use serde::{Deserialize, Deserializer};

use crate::fsutil;

/// A page of search results.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// How many projects match in all, across pages.
    pub total_hits: usize,
}

/// A project in search results.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    /// The owner's username.
    #[serde(default)]
    pub author: String,
    /// The one-line summary.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub icon_url: Option<String>,
    /// Every category, loaders included ("fabric", "optimization").
    #[serde(default)]
    pub categories: Vec<String>,
    /// The categories Modrinth shows for the result.
    #[serde(default)]
    pub display_categories: Vec<String>,
    #[serde(default)]
    pub client_side: Side,
    #[serde(default)]
    pub server_side: Side,
    pub date_modified: Timestamp,
}

/// A Modrinth project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Project {
    /// The permanent id, like `AANobbMI`.
    pub id: String,
    /// The name in its URL ("sodium"): unique, but its owner can change it.
    pub slug: String,
    pub title: String,
    /// The one-line summary.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub client_side: Side,
    #[serde(default)]
    pub server_side: Side,
    /// Every Minecraft version some version of the project supports.
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    pub updated: Timestamp,
}

/// Whether a mod runs on the client or the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Required,
    Optional,
    Unsupported,
    #[default]
    #[serde(other)]
    Unknown,
}

/// One release of a project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    /// The display name ("Sodium 0.6.13 for Fabric 1.21.4").
    #[serde(default)]
    pub name: String,
    /// The version string ("mc1.21.4-0.6.13-fabric").
    pub version_number: String,
    pub version_type: VersionType,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    // The Go app read this from `published`, so it never had it.
    pub date_published: Timestamp,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub files: Vec<VersionFile>,
}

impl Version {
    /// Whether it runs on Minecraft `game_version` with `loader`.
    pub fn supports(&self, game_version: &str, loader: &str) -> bool {
        self.game_versions.iter().any(|v| v == game_version)
            && self.loaders.iter().any(|l| l == loader)
    }

    /// The jar to install: the primary file if it's a jar, else the first jar.
    /// Files whose names aren't plain `.jar` file names (with a folder in them,
    /// say) are never picked.
    pub fn jar(&self) -> Option<&VersionFile> {
        let jars = || self.files.iter().filter(|f| is_jar_file_name(&f.filename));
        jars().find(|f| f.primary).or_else(|| jars().next())
    }
}

/// How stable a version is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Release,
    Beta,
    Alpha,
    #[serde(other)]
    Other,
}

/// Another project a version needs, works with, or conflicts with.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Dependency {
    pub project_id: Option<String>,
    /// A specific version, when the dependent pins one.
    pub version_id: Option<String>,
    /// A file outside Modrinth, for dependencies that aren't on it.
    pub file_name: Option<String>,
    pub dependency_type: DependencyType,
}

/// What a [`Dependency`] means for installing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    /// Must be installed too.
    Required,
    /// Adds features, but isn't needed.
    Optional,
    /// Mustn't be installed alongside.
    Incompatible,
    /// Bundled inside the dependent's jar.
    Embedded,
    #[serde(other)]
    Other,
}

/// A file attached to a version.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    /// Size in bytes.
    pub size: u64,
    pub hashes: FileHashes,
}

/// A file's published checksums, as lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileHashes {
    pub sha1: String,
}

/// The version to install on Minecraft `game_version` with `loader`: the
/// newest release that supports both and has a jar, else the newest such beta
/// or alpha. Newest means latest published, whatever order `versions` is in.
pub fn choose_version<'a>(
    versions: &'a [Version],
    game_version: &str,
    loader: &str,
) -> Option<&'a Version> {
    let candidates = || {
        versions
            .iter()
            .filter(|v| v.supports(game_version, loader) && v.jar().is_some())
    };
    candidates()
        .filter(|v| v.version_type == VersionType::Release)
        .max_by_key(|v| v.date_published)
        .or_else(|| candidates().max_by_key(|v| v.date_published))
}

/// Whether `name` is a jar's file name and nothing more: no folders, not
/// hidden, ending in `.jar`. Guards paths built from server-sent names.
pub(crate) fn is_jar_file_name(name: &str) -> bool {
    let stem = name.len().checked_sub(4).and_then(|split| {
        let (stem, ext) = (name.get(..split)?, name.get(split..)?);
        ext.eq_ignore_ascii_case(".jar").then_some(stem)
    });
    stem.is_some_and(|stem| !stem.is_empty())
        && !name.starts_with('.')
        && fsutil::is_plain_name(name)
}

/// Modrinth sends `""` for "no icon" in some places and `null` in others.
fn empty_as_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.filter(|s| !s.is_empty()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A release of `project` for 1.21.4 on Fabric, published on day `day` of
    /// 2025, with one primary jar named `<id>.jar`.
    pub(crate) fn version(project: &str, id: &str, day: u8) -> Version {
        Version {
            id: id.to_owned(),
            project_id: project.to_owned(),
            name: id.to_owned(),
            version_number: id.to_owned(),
            version_type: VersionType::Release,
            game_versions: vec!["1.21.4".to_owned()],
            loaders: vec!["fabric".to_owned()],
            date_published: format!("2025-01-{day:02}T00:00:00Z").parse().unwrap(),
            dependencies: Vec::new(),
            files: vec![file(&format!("{id}.jar"), true)],
        }
    }

    fn file(name: &str, primary: bool) -> VersionFile {
        VersionFile {
            url: format!("https://cdn.example/{name}"),
            filename: name.to_owned(),
            primary,
            size: 1,
            hashes: FileHashes {
                sha1: "0".repeat(40),
            },
        }
    }

    #[test]
    fn chooses_the_newest_compatible_release() {
        let v = version;
        let typed = |id: &str, day, version_type| Version {
            version_type,
            ..version("p", id, day)
        };
        let on = |id: &str, day, game: &str, loader: &str| Version {
            game_versions: vec![game.to_owned()],
            loaders: vec![loader.to_owned()],
            ..version("p", id, day)
        };
        let no_jar = Version {
            files: vec![file("readme.txt", true)],
            ..version("p", "no-jar", 30)
        };
        for (case, versions, want) in [
            ("none", vec![], None),
            (
                "newest by date, not list order",
                vec![v("p", "old", 1), v("p", "new", 9), v("p", "mid", 5)],
                Some("new"),
            ),
            (
                "a release beats a newer beta",
                vec![typed("beta", 9, VersionType::Beta), v("p", "rel", 1)],
                Some("rel"),
            ),
            (
                "the newest pre-release when there's no release",
                vec![
                    typed("alpha", 2, VersionType::Alpha),
                    typed("beta", 9, VersionType::Beta),
                ],
                Some("beta"),
            ),
            (
                "other game versions and loaders don't count",
                vec![
                    on("old-game", 9, "1.20.1", "fabric"),
                    on("forge", 9, "1.21.4", "forge"),
                    v("p", "ok", 1),
                ],
                Some("ok"),
            ),
            (
                "versions without a jar don't count",
                vec![no_jar, v("p", "ok", 1)],
                Some("ok"),
            ),
        ] {
            let got = choose_version(&versions, "1.21.4", "fabric").map(|v| v.id.as_str());
            assert_eq!(got, want, "{case}");
        }
    }

    #[test]
    fn picks_the_primary_jar_else_the_first() {
        let with = |files| Version {
            files,
            ..version("p", "v", 1)
        };
        for (case, files, want) in [
            (
                "primary jar",
                vec![file("a.jar", false), file("b.jar", true)],
                Some("b.jar"),
            ),
            (
                "first jar when the primary isn't one",
                vec![
                    file("sources.zip", true),
                    file("a.jar", false),
                    file("b.jar", false),
                ],
                Some("a.jar"),
            ),
            (
                "upper-case extension",
                vec![file("A.JAR", true)],
                Some("A.JAR"),
            ),
            ("no jar", vec![file("a.zip", true)], None),
            (
                "unsafe names are skipped",
                vec![
                    file("../evil.jar", true),
                    file("sub/evil.jar", true),
                    file(r"C:\evil.jar", true),
                    file(".hidden.jar", true),
                    file(".jar", true),
                    file("ok.jar", false),
                ],
                Some("ok.jar"),
            ),
        ] {
            let got = with(files).jar().map(|f| f.filename.clone());
            assert_eq!(got.as_deref(), want, "{case}");
        }
    }

    #[test]
    fn unknown_enum_values_decode() {
        let side: Side = serde_json::from_str(r#""someday""#).unwrap();
        assert_eq!(side, Side::Unknown);
        let dep: Dependency =
            serde_json::from_str(r#"{"dependency_type":"recommended","project_id":"x"}"#).unwrap();
        assert_eq!(dep.dependency_type, DependencyType::Other);
        assert_eq!(dep.version_id, None);
    }
}
