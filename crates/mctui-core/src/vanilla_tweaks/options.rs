//! Enabling the merged pack in Minecraft's `options.txt`, whose
//! `resourcePacks` line lists the enabled packs as JSON, lowest priority
//! first. From 1.13 packs are named `file/<zip>`, after the game's own:
//! `resourcePacks:["vanilla","file/VanillaTweaks.zip"]`. Before, they're
//! bare file names: `resourcePacks:["VanillaTweaks.zip"]`. Everything else in
//! the file is left as it was.

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::Path;

use super::{CatalogVersion, Error, PACK_FILE};
use crate::fsutil;

const FILE: &str = "options.txt";
const KEY: &str = "resourcePacks:";
/// How `options.txt` names the merged pack from 1.13.
const ENTRY: &str = "file/VanillaTweaks.zip";

/// Enables the merged pack on top of the instance's other packs, creating
/// `options.txt` if there isn't one.
pub(super) async fn enable(game_dir: &Path, version: CatalogVersion) -> Result<(), Error> {
    update(game_dir, move |options| with_pack(options, version)).await
}

/// Disables the merged pack, however it's named.
pub(super) async fn disable(game_dir: &Path) -> Result<(), Error> {
    update(game_dir, without_pack).await
}

/// Whether the instance's `options.txt` enables the merged pack, named as
/// the game version reads it.
pub(super) async fn is_enabled(game_dir: &Path, version: CatalogVersion) -> bool {
    tokio::fs::read_to_string(game_dir.join(FILE))
        .await
        .is_ok_and(|options| lists_pack(&options, version))
}

/// Rewrites `options.txt` with `edit`, which returns `None` to leave it as
/// it is. A missing file reads as empty.
async fn update(
    game_dir: &Path,
    edit: impl FnOnce(&str) -> Option<String> + Send + 'static,
) -> Result<(), Error> {
    super::blocking(game_dir.join(FILE), move |path| {
        let options = match fs::read_to_string(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            read => read?,
        };
        match edit(&options) {
            Some(edited) => fsutil::write_atomic(path, edited.as_bytes(), 0o644),
            None => Ok(()),
        }
    })
    .await
}

/// The merged pack's name in the list, for `version`.
fn entry(version: CatalogVersion) -> &'static str {
    if version.names_packs_by_file() {
        PACK_FILE
    } else {
        ENTRY
    }
}

/// Whether a listed pack is the merged pack, named either way.
fn is_merged_pack(pack: &str) -> bool {
    pack == ENTRY || pack == PACK_FILE
}

/// `options` with the merged pack last in the `resourcePacks` list, named
/// for `version`, or in a new line when there's no list. A list that isn't
/// valid JSON is replaced, and the pack named the other way (by an older
/// mctui) is taken out. `None` when the list is right already.
fn with_pack(options: &str, version: CatalogVersion) -> Option<String> {
    let entry = entry(version);
    if options.lines().any(|line| line.starts_with(KEY)) {
        return rewrite_lists(options, |packs| {
            let Some(listed) = packs else {
                return Some(default_list(version));
            };
            let mut packs: Vec<String> = listed
                .iter()
                .filter(|p| *p == entry || !is_merged_pack(p))
                .cloned()
                .collect();
            if !packs.iter().any(|p| p == entry) {
                packs.push(entry.to_owned());
            }
            (packs != listed).then_some(packs)
        });
    }
    // No list: add one, in the file's line endings.
    let newline = if options.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let separator = if options.is_empty() || options.ends_with('\n') {
        ""
    } else {
        newline
    };
    let list = encode(&default_list(version));
    Some(format!("{options}{separator}{KEY}{list}{newline}"))
}

/// `options` without the merged pack in the `resourcePacks` list, named
/// either way; `None` when it isn't listed. A list that isn't valid JSON is
/// left alone.
fn without_pack(options: &str) -> Option<String> {
    rewrite_lists(options, |packs| {
        let mut packs = packs?;
        let before = packs.len();
        packs.retain(|p| !is_merged_pack(p));
        (packs.len() < before).then_some(packs)
    })
}

/// Whether the `resourcePacks` list in `options` has the merged pack, named
/// for `version`.
fn lists_pack(options: &str, version: CatalogVersion) -> bool {
    let entry = entry(version);
    options
        .lines()
        .filter_map(|line| line.strip_prefix(KEY))
        .filter_map(parse)
        .any(|packs| packs.iter().any(|p| p == entry))
}

/// Passes each `resourcePacks` line's list to `edit` (`None` if it isn't a
/// JSON list of strings) and replaces the line, keeping its line ending, when
/// `edit` returns a new list. Returns the new text, or `None` when no line
/// changed. (Minecraft writes one such line; were there more, it would read
/// the last, so they're all edited.)
fn rewrite_lists(
    options: &str,
    mut edit: impl FnMut(Option<Vec<String>>) -> Option<Vec<String>>,
) -> Option<String> {
    let mut changed = false;
    let text: String = options
        .split_inclusive('\n')
        .map(|line| {
            let content = line.trim_end_matches(['\r', '\n']);
            let Some(value) = content.strip_prefix(KEY) else {
                return Cow::Borrowed(line);
            };
            match edit(parse(value)) {
                Some(packs) => {
                    changed = true;
                    let ending = &line[content.len()..];
                    Cow::Owned(format!("{KEY}{}{ending}", encode(&packs)))
                }
                None => Cow::Borrowed(line),
            }
        })
        .collect();
    changed.then_some(text)
}

/// A new list with just the merged pack, on the game's own from 1.13.
fn default_list(version: CatalogVersion) -> Vec<String> {
    if version.names_packs_by_file() {
        vec![PACK_FILE.to_owned()]
    } else {
        vec!["vanilla".to_owned(), ENTRY.to_owned()]
    }
}

fn parse(list: &str) -> Option<Vec<String>> {
    serde_json::from_str(list).ok()
}

/// A list as Minecraft writes it: compact JSON.
fn encode(packs: &[String]) -> String {
    serde_json::Value::from(packs).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = r#"resourcePacks:["vanilla","file/VanillaTweaks.zip"]"#;
    const LEGACY_DEFAULT: &str = r#"resourcePacks:["VanillaTweaks.zip"]"#;

    fn modern() -> CatalogVersion {
        CatalogVersion::for_minecraft("1.21.4").unwrap()
    }

    fn legacy() -> CatalogVersion {
        CatalogVersion::for_minecraft("1.12.2").unwrap()
    }

    #[test]
    fn entry_names_the_pack_file() {
        assert_eq!(ENTRY, format!("file/{PACK_FILE}"));
        assert_eq!(entry(modern()), ENTRY);
        assert_eq!(entry(legacy()), PACK_FILE);
    }

    #[test]
    fn enabling() {
        for (options, want) in [
            // No file yet.
            ("", Some(format!("{DEFAULT}\n"))),
            // No list: one is added, the rest kept.
            (
                "version:3955\nlang:en_us\n",
                Some(format!("version:3955\nlang:en_us\n{DEFAULT}\n")),
            ),
            ("version:3955", Some(format!("version:3955\n{DEFAULT}\n"))),
            (
                "version:3955\r\nlang:en_us\r\n",
                Some(format!("version:3955\r\nlang:en_us\r\n{DEFAULT}\r\n")),
            ),
            // A similar key isn't the list.
            (
                "incompatibleResourcePacks:[]\n",
                Some(format!("incompatibleResourcePacks:[]\n{DEFAULT}\n")),
            ),
            // Other packs are kept, and the merged pack goes on top.
            (
                "a:1\nresourcePacks:[\"vanilla\",\"file/A.zip\"]\nb:2\n",
                Some(format!(
                    "a:1\n{}\nb:2\n",
                    r#"resourcePacks:["vanilla","file/A.zip","file/VanillaTweaks.zip"]"#
                )),
            ),
            (
                "resourcePacks:[]",
                Some(r#"resourcePacks:["file/VanillaTweaks.zip"]"#.to_owned()),
            ),
            (
                "a:1\r\nresourcePacks:[\"vanilla\"]\r\nb:2\r\n",
                Some(format!("a:1\r\n{DEFAULT}\r\nb:2\r\n")),
            ),
            // Escapes Minecraft wrote are re-encoded, meaning the same.
            (
                "resourcePacks:[\"file/Tom\\u0027s.zip\"]",
                Some(r#"resourcePacks:["file/Tom's.zip","file/VanillaTweaks.zip"]"#.to_owned()),
            ),
            // A broken list is replaced.
            (
                "resourcePacks:[\"vanilla\",\n",
                Some(format!("{DEFAULT}\n")),
            ),
            // The pre-1.13 name is replaced by this version's.
            (
                "resourcePacks:[\"vanilla\",\"VanillaTweaks.zip\"]\n",
                Some(format!("{DEFAULT}\n")),
            ),
            // Already there: nothing to write.
            (DEFAULT, None),
            (
                "resourcePacks:[\"file/VanillaTweaks.zip\",\"file/Top.zip\"]\n",
                None,
            ),
        ] {
            check_enabling(modern(), options, want.as_deref());
        }
    }

    /// Before 1.13: bare file names, and no `vanilla` entry.
    #[test]
    fn enabling_before_1_13() {
        for (options, want) in [
            ("", Some(format!("{LEGACY_DEFAULT}\n"))),
            (
                "version:1343\nlang:en_us\n",
                Some(format!("version:1343\nlang:en_us\n{LEGACY_DEFAULT}\n")),
            ),
            (
                "resourcePacks:[\"Faithful.zip\"]\n",
                Some("resourcePacks:[\"Faithful.zip\",\"VanillaTweaks.zip\"]\n".to_owned()),
            ),
            ("resourcePacks:[]\n", Some(format!("{LEGACY_DEFAULT}\n"))),
            // What an older mctui wrote, which the game ignored.
            (
                "resourcePacks:[\"vanilla\",\"file/VanillaTweaks.zip\"]\n",
                Some("resourcePacks:[\"vanilla\",\"VanillaTweaks.zip\"]\n".to_owned()),
            ),
            (LEGACY_DEFAULT, None),
            ("resourcePacks:[\"VanillaTweaks.zip\",\"Top.zip\"]\n", None),
        ] {
            check_enabling(legacy(), options, want.as_deref());
        }
        assert!(
            !lists_pack(DEFAULT, legacy()),
            "1.12 can't read file/ names"
        );
        assert!(!lists_pack(LEGACY_DEFAULT, modern()));
    }

    fn check_enabling(version: CatalogVersion, options: &str, want: Option<&str>) {
        let got = with_pack(options, version);
        assert_eq!(got.as_deref(), want, "{version}: {options:?}");
        let enabled = got.as_deref().unwrap_or(options);
        assert!(lists_pack(enabled, version), "{version}: {enabled:?}");
        assert_eq!(
            with_pack(enabled, version),
            None,
            "idempotent: {version}: {enabled:?}"
        );
    }

    #[test]
    fn disabling() {
        for (options, want) in [
            (
                "a:1\nresourcePacks:[\"vanilla\",\"file/VanillaTweaks.zip\",\"file/A.zip\"]\nb:2\n",
                Some("a:1\nresourcePacks:[\"vanilla\",\"file/A.zip\"]\nb:2\n"),
            ),
            (
                "resourcePacks:[\"file/VanillaTweaks.zip\"]\r\n",
                Some("resourcePacks:[]\r\n"),
            ),
            // Named either way.
            (
                "resourcePacks:[\"Faithful.zip\",\"VanillaTweaks.zip\"]\n",
                Some("resourcePacks:[\"Faithful.zip\"]\n"),
            ),
            (
                "resourcePacks:[\"file/VanillaTweaks.zip\",\"VanillaTweaks.zip\"]\n",
                Some("resourcePacks:[]\n"),
            ),
            // Not listed, no list, no file, or a broken list: nothing to do.
            ("resourcePacks:[\"vanilla\"]\n", None),
            ("lang:en_us\n", None),
            ("", None),
            ("resourcePacks:[\"file/VanillaTweaks.zip\",\n", None),
        ] {
            let got = without_pack(options);
            assert_eq!(got.as_deref(), want, "{options:?}");
            if let Some(disabled) = &got {
                assert!(!lists_pack(disabled, modern()), "{disabled:?}");
                assert!(!lists_pack(disabled, legacy()), "{disabled:?}");
            }
        }
    }

    #[test]
    fn listing() {
        assert!(lists_pack(&format!("a:1\r\n{DEFAULT}\r\n"), modern()));
        assert!(!lists_pack("resourcePacks:[\"vanilla\"]\n", modern()));
        assert!(!lists_pack(
            "incompatibleResourcePacks:[\"file/VanillaTweaks.zip\"]\n",
            modern()
        ));
        assert!(!lists_pack("", modern()));
        assert!(lists_pack(&format!("a:1\n{LEGACY_DEFAULT}\n"), legacy()));
    }

    #[tokio::test]
    async fn edits_the_file_in_place() {
        for (version, enabled) in [(modern(), DEFAULT), (legacy(), LEGACY_DEFAULT)] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(FILE);

            assert!(!is_enabled(dir.path(), version).await);
            disable(dir.path()).await.unwrap();
            assert!(!path.exists(), "disabling doesn't create the file");

            enable(dir.path(), version).await.unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), format!("{enabled}\n"));
            assert!(is_enabled(dir.path(), version).await);

            disable(dir.path()).await.unwrap();
            assert!(!is_enabled(dir.path(), version).await);
        }
    }
}
