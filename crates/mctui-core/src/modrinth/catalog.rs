//! `mods/.mctui-modrinth.json`: which jars in the mods folder came from which
//! Modrinth project. The Go app wrote the same file:
//!
//! ```json
//! {"projects": [{"projectId": "AANobbMI", "slug": "sodium", "file": "sodium-fabric-0.6.13+mc1.21.4.jar"}]}
//! ```
//!
//! This version adds each project's `title`, which Go ignores.

use std::io;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

use crate::fsutil;

const FILE_NAME: &str = ".mctui-modrinth.json";

/// The whole file.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Catalog {
    // Go wrote `null` for "none".
    #[serde(default, deserialize_with = "null_as_empty")]
    pub projects: Vec<Entry>,
}

/// One installed project and its jar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Entry {
    pub project_id: String,
    #[serde(default)]
    pub slug: String,
    /// The jar's file name in the mods folder.
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl Entry {
    /// Whether `project_id` can be trusted. The Go app recorded its starter
    /// mods with the slug as the project id, so those entries are identified
    /// again from the jar.
    pub fn has_real_id(&self) -> bool {
        self.project_id != self.slug
    }
}

impl Catalog {
    /// Reads the catalog in `mods_dir`. A missing, unreadable, or damaged file
    /// reads as empty: it's only a record, and installs rebuild it.
    pub fn read(mods_dir: &Path) -> Self {
        let path = mods_dir.join(FILE_NAME);
        match crate::read_optional(&path) {
            Ok(Some(data)) => serde_json::from_slice(&data).unwrap_or_else(|e| {
                tracing::warn!(path = %path.display(), error = %e, "ignoring damaged mod catalog");
                Self::default()
            }),
            Ok(None) => Self::default(),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "couldn't read mod catalog");
                Self::default()
            }
        }
    }

    /// Reads the catalog to change it. A damaged file is moved aside, so the
    /// write that follows can't destroy what it held.
    pub fn read_for_update(mods_dir: &Path) -> io::Result<Self> {
        let path = mods_dir.join(FILE_NAME);
        let Some(data) = crate::read_optional(&path)? else {
            return Ok(Self::default());
        };
        serde_json::from_slice(&data).or_else(|e| {
            let kept = fsutil::quarantine(&path)?;
            tracing::warn!(error = %e, kept = %kept.display(), "replacing damaged mod catalog");
            Ok(Self::default())
        })
    }

    pub fn write(&self, mods_dir: &Path) -> io::Result<()> {
        let data = serde_json::to_vec_pretty(self)?;
        fsutil::write_atomic(&mods_dir.join(FILE_NAME), &data, 0o644)
    }

    /// The entry for the jar `file`.
    pub fn entry_for(&self, file: &str) -> Option<&Entry> {
        self.projects
            .iter()
            .find(|e| e.file.eq_ignore_ascii_case(file))
    }

    /// Adds `entry`, replacing any for the same project or the same jar.
    pub fn record(&mut self, entry: Entry) {
        self.projects.retain(|e| {
            e.project_id != entry.project_id && !e.file.eq_ignore_ascii_case(&entry.file)
        });
        self.projects.push(entry);
    }

    /// Drops the entries for the jar `file`; says whether there were any.
    pub fn forget(&mut self, file: &str) -> bool {
        let before = self.projects.len();
        self.projects.retain(|e| !e.file.eq_ignore_ascii_case(file));
        self.projects.len() != before
    }
}

fn null_as_empty<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Entry>, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project_id: &str, slug: &str, file: &str) -> Entry {
        Entry {
            project_id: project_id.to_owned(),
            slug: slug.to_owned(),
            file: file.to_owned(),
            title: None,
        }
    }

    #[test]
    fn reads_what_go_wrote() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE_NAME),
            r#"{
  "projects": [
    {
      "projectId": "fabric-api",
      "slug": "fabric-api",
      "file": "fabric-api-0.119.4+1.21.4.jar"
    },
    {
      "projectId": "AANobbMI",
      "slug": "sodium",
      "file": "sodium-fabric-0.6.13+mc1.21.4.jar"
    }
  ]
}"#,
        )
        .unwrap();
        let catalog = Catalog::read(dir.path());
        assert_eq!(
            catalog.projects,
            [
                entry("fabric-api", "fabric-api", "fabric-api-0.119.4+1.21.4.jar"),
                entry("AANobbMI", "sodium", "sodium-fabric-0.6.13+mc1.21.4.jar"),
            ]
        );
        assert!(!catalog.projects[0].has_real_id());
        assert!(catalog.projects[1].has_real_id());

        std::fs::write(dir.path().join(FILE_NAME), r#"{"projects": null}"#).unwrap();
        assert_eq!(Catalog::read(dir.path()), Catalog::default());
    }

    #[test]
    fn writes_go_keys_in_go_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::default();
        catalog.record(Entry {
            title: Some("Sodium".to_owned()),
            ..entry("AANobbMI", "sodium", "sodium.jar")
        });
        catalog.write(dir.path()).unwrap();
        let text = std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert_eq!(
            text,
            r#"{
  "projects": [
    {
      "projectId": "AANobbMI",
      "slug": "sodium",
      "file": "sodium.jar",
      "title": "Sodium"
    }
  ]
}"#
        );
    }

    #[test]
    fn record_replaces_by_project_and_by_file() {
        let mut catalog = Catalog {
            projects: vec![
                entry("A", "a", "a-1.jar"),
                entry("B", "b", "b.jar"),
                entry("C", "c", "c.jar"),
            ],
        };
        catalog.record(entry("A", "a", "a-2.jar"));
        catalog.record(entry("D", "d", "B.JAR"));
        let files: Vec<_> = catalog.projects.iter().map(|e| e.file.as_str()).collect();
        assert_eq!(files, ["c.jar", "a-2.jar", "B.JAR"]);

        assert!(catalog.forget("C.jar"));
        assert!(!catalog.forget("c.jar"));
        assert_eq!(
            catalog.entry_for("b.jar").map(|e| e.project_id.as_str()),
            Some("D")
        );
    }

    #[test]
    fn damaged_catalogs_read_as_empty_and_are_kept_on_update() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE_NAME), "{broken").unwrap();
        assert_eq!(Catalog::read(dir.path()), Catalog::default());
        assert!(dir.path().join(FILE_NAME).exists(), "reading moves nothing");

        assert_eq!(
            Catalog::read_for_update(dir.path()).unwrap(),
            Catalog::default()
        );
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            matches!(names.as_slice(), [kept] if kept.starts_with(".mctui-modrinth.json.bad-")),
            "{names:?}"
        );
    }
}
