//! Which Vanilla Tweaks catalog a Minecraft version uses.

use std::fmt;

/// The Minecraft versions Vanilla Tweaks publishes resource packs for, newest
/// first, as the site names them. Checked against the site in September 2026:
/// each has an `rpcategories.json`, and 1.9 and 1.10 never had one.
const PUBLISHED: [&str; 15] = [
    "26.3", "26.2", "26.1", "1.21", "1.20", "1.19", "1.18", "1.17", "1.16", "1.15", "1.14", "1.13",
    "1.12", "1.11", "1.8",
];
const OLDEST: &str = PUBLISHED[PUBLISHED.len() - 1];

/// A Minecraft version Vanilla Tweaks publishes packs for: `major.minor`, e.g.
/// "1.21" or "26.1".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CatalogVersion(&'static str);

impl CatalogVersion {
    /// The catalog for a Minecraft release: its own `major.minor` when
    /// published, else the newest older one. So "1.21.4" uses "1.21", "1.10.2"
    /// uses "1.8", and a release newer than every catalog uses the newest.
    pub fn for_minecraft(version: &str) -> Result<Self, Unsupported> {
        let version = version.trim();
        let Some(wanted) = release(version) else {
            return Err(if is_snapshot(version) {
                Unsupported::Snapshot
            } else {
                Unsupported::Unrecognized
            });
        };
        PUBLISHED
            .into_iter()
            .find(|published| release(published).is_some_and(|p| p <= wanted))
            .map(Self)
            .ok_or(Unsupported::TooOld)
    }

    /// The version as the site writes it, e.g. "1.21".
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for CatalogVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Why a Minecraft version has no Vanilla Tweaks packs. Displays as a sentence
/// for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unsupported {
    /// A snapshot, pre-release, or release candidate.
    #[error("Vanilla Tweaks doesn't make packs for snapshots or pre-releases.")]
    Snapshot,
    /// A release older than every catalog.
    #[error("Vanilla Tweaks packs start at Minecraft {OLDEST}.")]
    TooOld,
    /// Not a Minecraft version id this knows how to read.
    #[error("Vanilla Tweaks doesn't know this Minecraft version.")]
    Unrecognized,
}

/// `(major, minor)` of a release id: "1.21" or "1.21.4", digits only.
fn release(version: &str) -> Option<(u32, u32)> {
    let parts: Vec<Option<u32>> = version.split('.').map(number).collect();
    match parts[..] {
        [Some(major), Some(minor)] | [Some(major), Some(minor), Some(_)] => Some((major, minor)),
        _ => None,
    }
}

/// A run of ASCII digits as a number; "", "+1", and "1a" aren't.
fn number(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Weekly snapshots ("25w14a") and pre-releases of a release ("1.21.5-pre1",
/// "1.21-rc1", "26.1-snapshot-1", "1.14 Pre-Release 1").
fn is_snapshot(version: &str) -> bool {
    let weekly = matches!(
        version.as_bytes(),
        [y1, y2, b'w', w1, w2, _, ..] if [y1, y2, w1, w2].iter().all(|b| b.is_ascii_digit())
    );
    weekly
        || version
            .split_once(['-', ' '])
            .is_some_and(|(base, _)| release(base).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_minecraft_versions_to_catalogs() {
        for (minecraft, want) in [
            ("1.21", Ok("1.21")),
            ("1.21.4", Ok("1.21")),
            ("1.21.10", Ok("1.21")),
            (" 1.20.1 ", Ok("1.20")),
            ("1.16.5", Ok("1.16")),
            ("1.11.2", Ok("1.11")),
            // No 1.9 or 1.10 catalog: the nearest older one.
            ("1.10.2", Ok("1.8")),
            ("1.9", Ok("1.8")),
            ("1.8.9", Ok("1.8")),
            // The 2026 numbering.
            ("26.1", Ok("26.1")),
            ("26.2.1", Ok("26.2")),
            // Newer than every catalog: the newest.
            ("26.4", Ok("26.3")),
            ("27.1", Ok("26.3")),
            ("1.22", Ok("1.21")),
            ("1.7.10", Err(Unsupported::TooOld)),
            ("1.0", Err(Unsupported::TooOld)),
            ("25w14a", Err(Unsupported::Snapshot)),
            ("24w14potato", Err(Unsupported::Snapshot)),
            ("1.21.5-pre1", Err(Unsupported::Snapshot)),
            ("1.21-rc1", Err(Unsupported::Snapshot)),
            ("26.1-snapshot-1", Err(Unsupported::Snapshot)),
            ("1.14 Pre-Release 1", Err(Unsupported::Snapshot)),
            ("", Err(Unsupported::Unrecognized)),
            ("1", Err(Unsupported::Unrecognized)),
            ("1.x", Err(Unsupported::Unrecognized)),
            ("+1.21", Err(Unsupported::Unrecognized)),
            ("1.21.4.1", Err(Unsupported::Unrecognized)),
            ("b1.7.3", Err(Unsupported::Unrecognized)),
            ("rd-132211", Err(Unsupported::Unrecognized)),
            ("1.RV-Pre1", Err(Unsupported::Unrecognized)),
        ] {
            let got = CatalogVersion::for_minecraft(minecraft).map(CatalogVersion::as_str);
            assert_eq!(got, want, "{minecraft:?}");
        }
    }

    #[test]
    fn published_versions_are_releases_newest_first() {
        let parsed: Vec<_> = PUBLISHED.iter().map(|v| release(v)).collect();
        assert!(parsed.iter().all(Option::is_some), "{PUBLISHED:?}");
        assert!(parsed.is_sorted_by(|a, b| a > b), "{PUBLISHED:?}");
    }

    #[test]
    fn reasons_read_as_sentences() {
        assert_eq!(
            Unsupported::TooOld.to_string(),
            "Vanilla Tweaks packs start at Minecraft 1.8."
        );
        assert_eq!(
            CatalogVersion::for_minecraft("1.21.4").unwrap().to_string(),
            "1.21"
        );
    }
}
