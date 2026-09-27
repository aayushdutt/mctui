//! Cheap disk reads for the home screen's detail pane: an instance's worlds,
//! mods, and resource packs. Missing or unreadable folders read as empty.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use jiff::Timestamp;
use serde::Deserialize;

use crate::instance::Instance;

/// The merged pack the in-app Vanilla Tweaks browser writes.
const MERGED_PACK: &str = "VanillaTweaks.zip";
/// The Vanilla Tweaks cart, next to the merged pack.
const CART_FILE: &str = ".mctui-vanillatweaks.json";

/// A save folder under `.minecraft/saves`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    pub name: String,
    pub modified: Option<Timestamp>,
}

/// The instance's worlds, most recently modified first, then by name.
pub fn worlds(inst: &Instance) -> Vec<World> {
    let mut worlds: Vec<World> = visible_entries(&inst.game_dir().join("saves"))
        .filter(|(_, meta)| meta.is_dir())
        .map(|(name, meta)| World {
            name,
            modified: meta
                .modified()
                .ok()
                .and_then(|t| Timestamp::try_from(t).ok()),
        })
        .collect();
    worlds.sort_by_cached_key(|w| (Reverse(w.modified), w.name.to_lowercase()));
    worlds
}

/// A `.jar` directly in `.minecraft/mods`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModJar {
    pub file_name: String,
    /// A readable name parsed from the file name ("Fabric API").
    pub display_name: String,
    pub size: u64,
}

/// The instance's mod jars, sorted by file name.
pub fn mod_jars(inst: &Instance) -> Vec<ModJar> {
    let mut jars: Vec<ModJar> = visible_entries(&inst.game_dir().join("mods"))
        .filter(|(name, meta)| meta.is_file() && strip_ext(name, ".jar").is_some())
        .map(|(file_name, meta)| ModJar {
            display_name: parse_mod_file_name(&file_name).0,
            file_name,
            size: meta.len(),
        })
        .collect();
    jars.sort_by_cached_key(|j| j.file_name.to_lowercase());
    jars
}

/// An installed resource pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePack {
    pub name: String,
    /// Byte size, for `.zip` packs.
    pub size: Option<u64>,
    /// The Vanilla Tweaks category, for packs from the applied cart.
    pub category: Option<String>,
}

/// The instance's resource packs. Packs picked in the Vanilla Tweaks browser
/// are listed one by one (from the applied cart) instead of as the single
/// merged `VanillaTweaks.zip`, sorted by category then id; after them come the
/// `.zip` and folder packs dropped in by hand, by name.
pub fn resource_packs(inst: &Instance) -> Vec<ResourcePack> {
    let dir = inst.game_dir().join("resourcepacks");
    let mut packs: Vec<ResourcePack> = applied_cart(&dir.join(CART_FILE))
        .into_iter()
        .flat_map(|(category, mut ids)| {
            ids.sort();
            ids.into_iter().map(move |id| ResourcePack {
                name: humanize_pack_id(&id),
                size: None,
                category: Some(category.clone()),
            })
        })
        .collect();
    // The merged zip stands for the cart entries; list it only when the cart
    // is empty, so an installed pack is never hidden.
    let hide_merged = !packs.is_empty();

    let mut manual: Vec<ResourcePack> = visible_entries(&dir)
        .filter(|(name, _)| !(hide_merged && name.eq_ignore_ascii_case(MERGED_PACK)))
        .filter_map(|(name, meta)| {
            if meta.is_dir() {
                return Some(ResourcePack {
                    name,
                    size: None,
                    category: None,
                });
            }
            let stem = strip_ext(&name, ".zip")?;
            Some(ResourcePack {
                name: stem.replace('_', " "),
                size: Some(meta.len()),
                category: None,
            })
        })
        .collect();
    manual.sort_by_cached_key(|p| p.name.to_lowercase());
    packs.append(&mut manual);
    packs
}

/// The cart's last-applied selection: category -> pack ids. Anything wrong with
/// the file means no cart, as in the Go app.
fn applied_cart(path: &Path) -> BTreeMap<String, Vec<String>> {
    #[derive(Deserialize)]
    struct Cart {
        #[serde(default)]
        applied: BTreeMap<String, Vec<String>>,
    }
    fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice::<Cart>(&data).ok())
        .map(|cart| cart.applied)
        .unwrap_or_default()
}

/// Non-hidden entries of `dir` with their metadata (following symlinks).
fn visible_entries(dir: &Path) -> impl Iterator<Item = (String, fs::Metadata)> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let meta = fs::metadata(entry.path()).ok()?;
            Some((name, meta))
        })
}

/// `name` without `ext`, matched case-insensitively.
fn strip_ext<'a>(name: &'a str, ext: &str) -> Option<&'a str> {
    let split = name.len().checked_sub(ext.len())?;
    let (stem, tail) = (name.get(..split)?, name.get(split..)?);
    tail.eq_ignore_ascii_case(ext).then_some(stem)
}

/// Turns a Vanilla Tweaks pack id into a label by splitting it at case
/// changes, `_`, and `-`: `BorderlessGlass` reads "Borderless Glass". (The
/// catalog's nicer names need the network.)
fn humanize_pack_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len() + 4);
    let mut prev: Option<char> = None;
    for c in id
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
    {
        if c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
            out.push(' ');
        }
        out.push(c);
        prev = Some(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Loader names that trail a mod's name in jar file names ("sodium-fabric-…").
const LOADER_WORDS: [&str; 10] = [
    "fabric",
    "quilt",
    "forge",
    "neoforge",
    "babric",
    "ornithe",
    "rift",
    "liteloader",
    "modloader",
    "nilloader",
];

/// Short words kept upper-case when a lower-case file name is title-cased.
const ACRONYMS: [&str; 7] = ["api", "yacl", "emf", "etf", "fps", "ui", "hud"];

/// Parses a mod jar's file name into a readable name and version, as the Go
/// mods screen did:
///
/// - `sodium-fabric-0.9.1+mc26.2.jar` → "Sodium", "0.9.1"
/// - `fabric-api-0.154.2+26.2.jar` → "Fabric API", "0.154.2"
/// - `Xaeros_Minimap_24.1.0_Fabric_1.21.jar` → "Xaeros Minimap", "24.1.0"
///
/// The name is the words (split at `-`, `_`, and spaces) before the first
/// version-like one, minus trailing loader words; lower-case names are title
/// cased. The version is the first word from there that starts with a digit,
/// up to any `+build` suffix.
fn parse_mod_file_name(file: &str) -> (String, Option<String>) {
    let base = strip_ext(file, ".jar").unwrap_or(file);
    let words: Vec<&str> = base
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .collect();
    let split = words
        .iter()
        .skip(1)
        .position(|w| is_version_word(w) || is_mc_version_word(w))
        .map_or(words.len(), |i| i + 1);
    let (mut name, rest) = words.split_at(split);
    while let [init @ .., last] = name
        && !init.is_empty()
        && LOADER_WORDS.contains(&last.to_lowercase().as_str())
    {
        name = init;
    }
    let version = rest.iter().find(|w| is_version_word(w)).map(|w| {
        let w = w.strip_prefix('v').unwrap_or(w);
        w.split('+').next().unwrap_or(w).to_owned()
    });

    let joined = name.join(" ");
    let display = if joined == joined.to_lowercase() {
        name.iter()
            .map(|w| title_case(w))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        joined
    };
    if display.trim().is_empty() {
        (base.to_owned(), version)
    } else {
        (display, version)
    }
}

fn title_case(word: &str) -> String {
    if ACRONYMS.contains(&word) {
        return word.to_uppercase();
    }
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// "0.9.1+mc26.2", "v15.0.140".
fn is_version_word(w: &str) -> bool {
    w.strip_prefix('v')
        .unwrap_or(w)
        .starts_with(|c: char| c.is_ascii_digit())
}

/// "mc1.21.4": ends a mod's name but isn't its version.
fn is_mc_version_word(w: &str) -> bool {
    w.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("mc"))
        && w[2..].starts_with(|c: char| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::instance::{InstanceStore, NewInstance};
    use crate::paths::DataDir;

    fn instance() -> (tempfile::TempDir, Instance) {
        let dir = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(&DataDir::new(dir.path()));
        let inst = store
            .create(NewInstance {
                name: "test".to_owned(),
                ..NewInstance::default()
            })
            .unwrap();
        (dir, inst)
    }

    fn mkdir(path: &Path) {
        fs::create_dir_all(path).unwrap();
    }

    fn touch(path: &Path, bytes: usize) {
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn missing_dirs_are_empty() {
        let (_dir, inst) = instance();
        assert!(worlds(&inst).is_empty());
        assert!(mod_jars(&inst).is_empty());
        assert!(resource_packs(&inst).is_empty());
    }

    #[test]
    fn worlds_newest_first() {
        let (_dir, inst) = instance();
        let saves = inst.game_dir().join("saves");
        let now = SystemTime::now();
        for (name, age_secs) in [("Old", 300), ("New", 10), ("Mid", 100)] {
            mkdir(&saves.join(name));
            let dir = fs::File::open(saves.join(name)).unwrap();
            dir.set_modified(now - Duration::from_secs(age_secs))
                .unwrap();
        }
        mkdir(&saves.join(".hidden"));
        touch(&saves.join("notes.txt"), 1);

        let names: Vec<_> = worlds(&inst).into_iter().map(|w| w.name).collect();
        assert_eq!(names, ["New", "Mid", "Old"]);
    }

    #[test]
    fn mod_jars_are_sorted_and_named() {
        let (_dir, inst) = instance();
        let mods = inst.game_dir().join("mods");
        mkdir(&mods.join("subdir.jar"));
        touch(&mods.join("sodium-fabric-0.9.1+mc26.2.jar"), 3);
        touch(&mods.join("fabric-api-0.154.2+26.2.JAR"), 5);
        touch(&mods.join("readme.txt"), 1);
        touch(&mods.join(".mctui-modrinth.json"), 1);

        let jars = mod_jars(&inst);
        let got: Vec<_> = jars
            .iter()
            .map(|j| (j.display_name.as_str(), j.size))
            .collect();
        assert_eq!(got, [("Fabric API", 5), ("Sodium", 3)]);
    }

    #[test]
    fn mod_file_names_parse_like_go() {
        for (file, name, version) in [
            ("sodium-fabric-0.9.1+mc26.2.jar", "Sodium", Some("0.9.1")),
            ("fabric-api-0.154.2+26.2.jar", "Fabric API", Some("0.154.2")),
            (
                "ImmediatelyFast-Fabric-1.17.1+26.3.jar",
                "ImmediatelyFast",
                Some("1.17.1"),
            ),
            (
                "lithium-fabric-mc1.21.4-0.14.3.jar",
                "Lithium",
                Some("0.14.3"),
            ),
            ("modmenu-17.0.0.jar", "Modmenu", Some("17.0.0")),
            (
                "cloth-config-v15.0.140.jar",
                "Cloth Config",
                Some("15.0.140"),
            ),
            ("sodium.jar", "Sodium", None),
            ("fabric.jar", "Fabric", None),
            (
                "Xaeros_Minimap_24.1.0_Fabric_1.21.jar",
                "Xaeros Minimap",
                Some("24.1.0"),
            ),
            ("1.0.jar", "1.0", None),
        ] {
            let got = parse_mod_file_name(file);
            assert_eq!(
                (got.0.as_str(), got.1.as_deref()),
                (name, version),
                "{file}"
            );
        }
    }

    #[test]
    fn pack_ids_are_humanized() {
        assert_eq!(humanize_pack_id("BorderlessGlass"), "Borderless Glass");
        assert_eq!(humanize_pack_id("blue_netherrack"), "blue netherrack");
        assert_eq!(humanize_pack_id("Better-Lava2X"), "Better Lava2 X");
        assert_eq!(humanize_pack_id("HD"), "HD");
    }

    #[test]
    fn resource_packs_without_cart_list_the_merged_zip() {
        let (_dir, inst) = instance();
        let rp = inst.game_dir().join("resourcepacks");
        mkdir(&rp.join("UnzippedPack"));
        touch(&rp.join("Faithful_32x.zip"), 8);
        touch(&rp.join(MERGED_PACK), 4);
        touch(&rp.join("notes.txt"), 1);
        touch(&rp.join(".DS_Store"), 1);

        let packs = resource_packs(&inst);
        let got: Vec<_> = packs.iter().map(|p| (p.name.as_str(), p.size)).collect();
        assert_eq!(
            got,
            [
                ("Faithful 32x", Some(8)),
                ("UnzippedPack", None),
                ("VanillaTweaks", Some(4)),
            ]
        );
    }

    #[test]
    fn resource_packs_expand_the_applied_cart() {
        let (_dir, inst) = instance();
        let rp = inst.game_dir().join("resourcepacks");
        mkdir(&rp);
        touch(&rp.join(MERGED_PACK), 4);
        touch(&rp.join("Faithful.zip"), 2);
        fs::write(
            rp.join(CART_FILE),
            r#"{
              "version": "1.21",
              "packs": {"Terrain": ["BlueNetherrack"], "Aesthetic": ["Pending"]},
              "applied": {"Terrain": ["BlueNetherrack"], "Aesthetic": ["ZTweak", "BorderlessGlass"]}
            }"#,
        )
        .unwrap();

        let packs = resource_packs(&inst);
        let got: Vec<_> = packs
            .iter()
            .map(|p| (p.name.as_str(), p.category.as_deref(), p.size))
            .collect();
        assert_eq!(
            got,
            [
                ("Borderless Glass", Some("Aesthetic"), None),
                ("ZTweak", Some("Aesthetic"), None),
                ("Blue Netherrack", Some("Terrain"), None),
                ("Faithful", None, Some(2)),
            ]
        );
    }

    #[test]
    fn corrupt_cart_is_no_cart() {
        let (_dir, inst) = instance();
        let rp = inst.game_dir().join("resourcepacks");
        mkdir(&rp);
        touch(&rp.join(MERGED_PACK), 4);
        fs::write(rp.join(CART_FILE), "{not json").unwrap();
        let names: Vec<_> = resource_packs(&inst).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["VanillaTweaks"]);
    }
}
