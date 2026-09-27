//! The version manifest: every game version Mojang publishes.

use std::fmt;

use jiff::Timestamp;
use serde::Deserialize;

/// Mojang's list of every game version, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    /// The newest release and snapshot.
    pub latest: Latest,
    /// Every version, newest first.
    pub versions: Vec<VersionEntry>,
}

impl Manifest {
    /// The entry for version `id`, e.g. `1.21.8`.
    pub fn find(&self, id: &str) -> Option<&VersionEntry> {
        self.versions.iter().find(|v| v.id == id)
    }
}

/// The newest release and snapshot ids.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Latest {
    /// The newest release's id.
    pub release: String,
    /// The newest snapshot's id, or the release's when that came out later.
    pub snapshot: String,
}

/// One game version in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    /// e.g. `1.21.8` or `24w14a`.
    pub id: String,
    /// Release, snapshot, and so on.
    #[serde(rename = "type")]
    pub kind: VersionKind,
    /// Where the version's JSON is.
    pub url: String,
    /// SHA-1 of the version's JSON.
    pub sha1: String,
    /// When the version came out, for sorting and display.
    pub release_time: Timestamp,
}

/// The kind of a game version.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum VersionKind {
    /// A full release.
    Release,
    /// Snapshots, pre-releases, and release candidates.
    Snapshot,
    /// Beta 1.0 to 1.8.1.
    OldBeta,
    /// Alpha and earlier.
    OldAlpha,
    /// A kind this launcher doesn't know, kept verbatim.
    Other(String),
}

impl VersionKind {
    /// Mojang's name for it, e.g. `old_beta`; also the `${version_type}`
    /// argument.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Release => "release",
            Self::Snapshot => "snapshot",
            Self::OldBeta => "old_beta",
            Self::OldAlpha => "old_alpha",
            Self::Other(kind) => kind,
        }
    }
}

impl From<String> for VersionKind {
    fn from(kind: String) -> Self {
        match kind.as_str() {
            "release" => Self::Release,
            "snapshot" => Self::Snapshot,
            "old_beta" => Self::OldBeta,
            "old_alpha" => Self::OldAlpha,
            _ => Self::Other(kind),
        }
    }
}

impl fmt::Display for VersionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_and_keep_unknowns() {
        for (name, kind) in [
            ("release", VersionKind::Release),
            ("snapshot", VersionKind::Snapshot),
            ("old_beta", VersionKind::OldBeta),
            ("old_alpha", VersionKind::OldAlpha),
            ("pending", VersionKind::Other("pending".into())),
        ] {
            let parsed: VersionKind = serde_json::from_str(&format!("{name:?}")).unwrap();
            assert_eq!(parsed, kind);
            assert_eq!(parsed.as_str(), name);
        }
    }
}
