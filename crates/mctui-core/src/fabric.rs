//! Fabric: which game and loader versions meta.fabricmc.net offers, and a
//! game version with the Fabric loader merged in, ready to launch.

use std::cmp::Ordering;
use std::path::PathBuf;

use reqwest::StatusCode;
use serde::Deserialize;

use crate::mojang::maven::Coordinate;
use crate::mojang::rules::compare_versions;
use crate::mojang::{self, CacheError, Fetched, Library, Mojang, VersionJson, cache};
use crate::net::{self, Http};
use crate::paths::DataDir;

/// Fabric's metadata service.
pub const META_URL: &str = "https://meta.fabricmc.net";

/// Why Fabric metadata couldn't be had.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A version list couldn't be fetched.
    #[error("couldn't get Fabric's versions: {0}")]
    Meta(#[source] net::Error),
    /// A profile couldn't be fetched, and none is cached.
    #[error("couldn't get Fabric {loader} for Minecraft {game}: {source}")]
    Profile {
        game: String,
        loader: String,
        #[source]
        source: net::Error,
    },
    /// Fabric doesn't know this game or loader version.
    #[error("Fabric {loader} isn't available for Minecraft {game}")]
    Unavailable { game: String, loader: String },
    /// Fabric sent a profile for some other game version.
    #[error("Fabric's profile for {loader} doesn't build on Minecraft {game}")]
    WrongGame { game: String, loader: String },
    /// The game version itself couldn't be had.
    #[error(transparent)]
    Mojang(#[from] mojang::Error),
    /// A cached profile exists but can't be used.
    #[error(transparent)]
    Cache(#[from] CacheError),
}

/// A result with this module's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A game version Fabric supports.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GameVersion {
    /// The game version id, as in Mojang's manifest.
    pub version: String,
    /// A release, as opposed to a snapshot.
    pub stable: bool,
}

/// A version of the Fabric loader.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LoaderVersion {
    /// e.g. `0.19.5`.
    pub version: String,
    /// Recommended for players, as opposed to a beta.
    pub stable: bool,
}

/// The loader version to use when none was chosen: the newest stable one,
/// else the newest. `versions` is newest first, as Fabric lists them.
pub fn default_loader(versions: &[LoaderVersion]) -> Option<&LoaderVersion> {
    versions
        .iter()
        .find(|v| v.stable)
        .or_else(|| versions.first())
}

/// Talks to Fabric's metadata service and caches loader profiles. Cheap to
/// clone.
#[derive(Debug, Clone)]
pub struct Fabric {
    http: Http,
    meta_url: String,
    versions_dir: PathBuf,
}

impl Fabric {
    /// A client for meta.fabricmc.net, caching under `data`'s cache dir.
    pub fn new(http: Http, data: &DataDir) -> Self {
        Self::with_meta_url(http, data, META_URL)
    }

    /// A client for the metadata service at `meta_url`.
    pub fn with_meta_url(http: Http, data: &DataDir, meta_url: impl Into<String>) -> Self {
        let meta_url = meta_url.into().trim_end_matches('/').to_owned();
        Self {
            http,
            meta_url,
            versions_dir: cache::versions_dir(data),
        }
    }

    /// The game versions Fabric supports, newest first.
    pub async fn game_versions(&self) -> Result<Vec<GameVersion>> {
        let url = self.url(&["v2", "versions", "game"]);
        self.http.get_json(&url).await.map_err(Error::Meta)
    }

    /// The loader versions for `game`, newest first. Empty when Fabric
    /// doesn't support `game`.
    pub async fn loader_versions(&self, game: &str) -> Result<Vec<LoaderVersion>> {
        #[derive(Deserialize)]
        struct Entry {
            loader: LoaderVersion,
        }
        let url = self.url(&["v2", "versions", "loader", game]);
        match self.http.get_json::<Vec<Entry>>(&url).await {
            Ok(entries) => Ok(entries.into_iter().map(|e| e.loader).collect()),
            // Fabric's answer for a game version it doesn't know.
            Err(e) if e.status() == Some(StatusCode::BAD_REQUEST) => Ok(Vec::new()),
            Err(e) => Err(Error::Meta(e)),
        }
    }

    /// Fabric's profile for `loader` on `game`: a partial version JSON that
    /// builds on `game`. A profile never changes, so it's fetched once and
    /// then read from `cache/versions/fabric-loader-<loader>-<game>.json`,
    /// online or not.
    pub async fn profile(&self, game: &str, loader: &str) -> Result<VersionJson> {
        let unavailable = || Error::Unavailable {
            game: game.to_owned(),
            loader: loader.to_owned(),
        };
        let path = cache::file(
            &self.versions_dir,
            &format!("fabric-loader-{loader}-{game}"),
        )
        .ok_or_else(unavailable)?;
        match cache::read(&path).await {
            Ok(Some(profile)) => return builds_on(profile, game, loader),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "fetching the Fabric profile again"),
        }
        let url = self.url(&["v2", "versions", "loader", game, loader, "profile", "json"]);
        match cache::fetch(&self.http, &url).await {
            Ok((profile, raw)) => {
                let profile = builds_on(profile, game, loader)?;
                cache::save(path, raw).await;
                Ok(profile)
            }
            // Fabric's answer for a game or loader version it doesn't know.
            Err(e) if e.status() == Some(StatusCode::BAD_REQUEST) => Err(unavailable()),
            Err(source) => Err(Error::Profile {
                game: game.to_owned(),
                loader: loader.to_owned(),
                source,
            }),
        }
    }

    /// Game version `game` with Fabric `loader` merged in, ready to launch:
    /// the game's JSON from [`Mojang::resolve`] and the loader's
    /// [`profile`](Self::profile), combined by [`merge`]. Works offline once
    /// both were fetched; stale when the game's JSON came from the cache
    /// after fetching failed.
    pub async fn resolve(
        &self,
        mojang: &Mojang,
        game: &str,
        loader: &str,
    ) -> Result<Fetched<VersionJson>> {
        let (game_json, profile) = tokio::try_join!(
            async { Ok(mojang.resolve(game).await?) },
            self.profile(game, loader),
        )?;
        Ok(Fetched {
            value: merge(game_json.value, profile),
            stale: game_json.stale,
        })
    }

    fn url(&self, path: &[&str]) -> String {
        path.iter().fold(self.meta_url.clone(), |mut url, segment| {
            url.push('/');
            url.push_str(&encode_segment(segment));
            url
        })
    }
}

/// Checks that `profile` is for `game` before it's used or cached.
fn builds_on(profile: VersionJson, game: &str, loader: &str) -> Result<VersionJson> {
    if profile.inherits_from.as_deref() == Some(game) {
        Ok(profile)
    } else {
        Err(Error::WrongGame {
            game: game.to_owned(),
            loader: loader.to_owned(),
        })
    }
}

/// Percent-encodes all but RFC 3986's unreserved characters, so versions
/// like `1.14 Pre-Release 5` and `0.4.8+build.155` stay one path segment.
fn encode_segment(segment: &str) -> String {
    use std::fmt::Write as _;
    segment
        .bytes()
        .fold(String::with_capacity(segment.len()), |mut out, b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                out.push(char::from(b));
            } else {
                let _ = write!(out, "%{b:02X}");
            }
            out
        })
}

/// Applies `child`, a loader profile, on top of `parent`, the game version it
/// builds on. The result keeps the parent's identity (id, type, assets,
/// downloads, Java, logging), with:
///
/// - the child's main class;
/// - argument lists joined, the parent's first;
/// - the parent's libraries minus those the child replaces, then the child's.
///
/// A child library replaces the parent's copies of the same library (group,
/// artifact, classifier), unless the parent's is newer: the game may need
/// the newer one. Modular ASM also replaces the all-in-one `asm-all` jar of
/// LaunchWrapper-era versions (1.5.2 and older), whose classes it duplicates.
///
/// Parent libraries never replace each other. A version JSON often lists one
/// library twice under different rules (1.14–1.18 have LWJGL 3.2.1 for macOS
/// and 3.2.2 for everything else), and the rules need both to choose from.
pub fn merge(parent: VersionJson, child: VersionJson) -> VersionJson {
    let mut merged = parent;
    merged.main_class = child.main_class;
    let args = &mut merged.arguments;
    args.game.extend(child.arguments.game);
    args.jvm.extend(child.arguments.jvm);
    args.default_user_jvm
        .extend(child.arguments.default_user_jvm);
    merged.libraries = merge_libraries(merged.libraries, child.libraries);
    merged
}

fn merge_libraries(parent: Vec<Library>, child: Vec<Library>) -> Vec<Library> {
    let child: Vec<Library> = child
        .into_iter()
        .filter(|new| !parent.iter().any(|old| is_newer_copy(old, new)))
        .collect();
    let mut merged: Vec<Library> = parent
        .into_iter()
        .filter(|old| !child.iter().any(|new| replaces(new, old)))
        .collect();
    merged.extend(child);
    merged
}

/// Whether `old` is the same library as `new`, in a newer version.
fn is_newer_copy(old: &Library, new: &Library) -> bool {
    let (Ok(old), Ok(new)) = (Coordinate::parse(&old.name), Coordinate::parse(&new.name)) else {
        return false;
    };
    old.same_library(&new) && compare_versions(old.version, new.version) == Some(Ordering::Greater)
}

/// Whether child library `new` takes the place of parent library `old`.
fn replaces(new: &Library, old: &Library) -> bool {
    const ASM: &str = "org.ow2.asm";
    let (Ok(new), Ok(old)) = (Coordinate::parse(&new.name), Coordinate::parse(&old.name)) else {
        return false;
    };
    new.same_library(&old) || (old.group == ASM && old.artifact == "asm-all" && new.group == ASM)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(name: &str) -> Library {
        serde_json::from_value(serde_json::json!({ "name": name })).unwrap()
    }

    fn names(libs: &[Library]) -> Vec<&str> {
        libs.iter().map(|l| l.name.as_str()).collect()
    }

    fn merged_names(parent: &[&str], child: &[&str]) -> Vec<String> {
        let libs = |names: &[&str]| names.iter().map(|n| lib(n)).collect();
        let merged = merge_libraries(libs(parent), libs(child));
        names(&merged).into_iter().map(str::to_owned).collect()
    }

    /// What's tested, the parent's libraries, the child's, and the result.
    type Case = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
        &'static [&'static str],
    );

    #[test]
    fn library_merging() {
        let cases: &[Case] = &[
            (
                "child replaces an older parent copy",
                &["org.ow2.asm:asm:9.6", "com.mojang:brigadier:1.3.10"],
                &[
                    "org.ow2.asm:asm:9.10.1",
                    "net.fabricmc:fabric-loader:0.19.5",
                ],
                &[
                    "com.mojang:brigadier:1.3.10",
                    "org.ow2.asm:asm:9.10.1",
                    "net.fabricmc:fabric-loader:0.19.5",
                ],
            ),
            (
                "child never downgrades",
                &["org.ow2.asm:asm:9.10.1"],
                &["org.ow2.asm:asm:9.6"],
                &["org.ow2.asm:asm:9.10.1"],
            ),
            (
                "the same version isn't listed twice",
                &["org.ow2.asm:asm:9.6"],
                &["org.ow2.asm:asm:9.6"],
                &["org.ow2.asm:asm:9.6"],
            ),
            (
                "versions that aren't numbers: the child wins",
                &["g:a:1.0-SNAPSHOT"],
                &["g:a:0.9+build.1"],
                &["g:a:0.9+build.1"],
            ),
            (
                "classifiers are separate libraries",
                &["g:a:1:natives-linux"],
                &["g:a:2:natives-osx"],
                &["g:a:1:natives-linux", "g:a:2:natives-osx"],
            ),
            (
                "the parent's duplicates are all kept (L-6)",
                &["org.lwjgl:lwjgl:3.2.1", "org.lwjgl:lwjgl:3.2.2"],
                &["net.fabricmc:fabric-loader:0.19.5"],
                &[
                    "org.lwjgl:lwjgl:3.2.1",
                    "org.lwjgl:lwjgl:3.2.2",
                    "net.fabricmc:fabric-loader:0.19.5",
                ],
            ),
            (
                "modular ASM replaces asm-all",
                &["org.ow2.asm:asm-all:4.1", "net.minecraft:launchwrapper:1.5"],
                &["org.ow2.asm:asm-tree:9.6"],
                &[
                    "net.minecraft:launchwrapper:1.5",
                    "org.ow2.asm:asm-tree:9.6",
                ],
            ),
            (
                "asm-all stays without modular ASM",
                &["org.ow2.asm:asm-all:4.1"],
                &["net.fabricmc:fabric-loader:0.19.5"],
                &[
                    "org.ow2.asm:asm-all:4.1",
                    "net.fabricmc:fabric-loader:0.19.5",
                ],
            ),
            (
                "names that aren't coordinates are kept",
                &["weird"],
                &["weird"],
                &["weird", "weird"],
            ),
        ];
        for (name, parent, child, want) in cases {
            assert_eq!(merged_names(parent, child), *want, "{name}");
        }
    }

    #[test]
    fn merge_keeps_the_parents_identity() {
        let parent: VersionJson = serde_json::from_value(serde_json::json!({
            "id": "1.21.8", "type": "release", "mainClass": "net.minecraft.client.main.Main",
            "assets": "26", "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21},
            "arguments": {"game": ["--username", "${auth_player_name}"], "jvm": ["-cp", "${classpath}"]},
            "libraries": [{"name": "org.ow2.asm:asm:9.6"}]
        }))
        .unwrap();
        let child: VersionJson = serde_json::from_value(serde_json::json!({
            "id": "fabric-loader-0.19.5-1.21.8", "inheritsFrom": "1.21.8", "type": "release",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {"game": ["--fabric"], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "]},
            "libraries": [{"name": "net.fabricmc:fabric-loader:0.19.5", "url": "https://maven.fabricmc.net/"}]
        }))
        .unwrap();
        let merged = merge(parent.clone(), child);

        assert_eq!(merged.id, "1.21.8");
        assert_eq!(merged.inherits_from, None);
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(merged.assets, parent.assets);
        assert_eq!(merged.java_version, parent.java_version);
        let values = |args: &[mojang::Argument]| -> Vec<String> {
            args.iter().flat_map(|a| a.values.clone()).collect()
        };
        assert_eq!(
            values(&merged.arguments.game),
            ["--username", "${auth_player_name}", "--fabric"]
        );
        assert_eq!(
            values(&merged.arguments.jvm),
            [
                "-cp",
                "${classpath}",
                "-DFabricMcEmu= net.minecraft.client.main.Main "
            ]
        );
        assert_eq!(
            names(&merged.libraries),
            ["org.ow2.asm:asm:9.6", "net.fabricmc:fabric-loader:0.19.5"]
        );
    }

    #[test]
    fn default_loader_prefers_the_newest_stable() {
        let v = |version: &str, stable| LoaderVersion {
            version: version.into(),
            stable,
        };
        let cases = [
            (
                vec![
                    v("0.20.0-beta.1", false),
                    v("0.19.5", true),
                    v("0.19.4", true),
                ],
                Some("0.19.5"),
            ),
            (
                vec![v("0.20.0-beta.2", false), v("0.20.0-beta.1", false)],
                Some("0.20.0-beta.2"),
            ),
            (vec![], None),
        ];
        for (versions, want) in cases {
            assert_eq!(default_loader(&versions).map(|v| v.version.as_str()), want);
        }
    }

    #[test]
    fn segments_are_encoded() {
        assert_eq!(encode_segment("1.21.8"), "1.21.8");
        assert_eq!(
            encode_segment("1.14 Pre-Release 5"),
            "1.14%20Pre-Release%205"
        );
        assert_eq!(encode_segment("0.4.8+build.155"), "0.4.8%2Bbuild.155");
        assert_eq!(encode_segment("a/b"), "a%2Fb");
        assert_eq!(encode_segment("20w14∞"), "20w14%E2%88%9E");
    }
}
