//! What's in an instance's mods folder, and removing it. Disk only.

use super::catalog::Catalog;
use super::model::is_jar_file_name;
use super::{Error, Result, mods_dir, remove_if_present};
use crate::facets;
use crate::fsutil::blocking;
use crate::instance::Instance;

/// A jar in an instance's mods folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledMod {
    /// The jar's file name.
    pub file: String,
    /// The Modrinth title when mctui installed it, else a name read from the
    /// file name.
    pub title: String,
    /// The version read from the file name ("0.6.13"), if it has one.
    pub version: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// The Modrinth project, when mctui installed it. (The Go app recorded its
    /// starter mods with the slug here.)
    pub project_id: Option<String>,
    pub slug: Option<String>,
}

impl InstalledMod {
    /// Whether this is the project with this id and slug, as in a search hit.
    pub fn is_project(&self, id: &str, slug: &str) -> bool {
        self.project_id.as_deref() == Some(id) || self.slug.as_deref() == Some(slug)
    }
}

/// The jars in the instance's mods folder, by title.
pub async fn installed(inst: &Instance) -> Vec<InstalledMod> {
    let inst = inst.clone();
    let listed = blocking(move || {
        let catalog = Catalog::read(&mods_dir(&inst));
        let mut mods: Vec<InstalledMod> = facets::mod_jars(&inst)
            .into_iter()
            .map(|jar| {
                let entry = catalog.entry_for(&jar.file_name);
                InstalledMod {
                    title: entry
                        .and_then(|e| e.title.clone())
                        .unwrap_or(jar.display_name),
                    version: facets::parse_mod_file_name(&jar.file_name).1,
                    size: jar.size,
                    project_id: entry.map(|e| e.project_id.clone()),
                    slug: entry.map(|e| e.slug.clone()).filter(|s| !s.is_empty()),
                    file: jar.file_name,
                }
            })
            .collect();
        mods.sort_by_cached_key(|m| (m.title.to_lowercase(), m.file.clone()));
        Ok::<_, std::io::Error>(mods)
    });
    listed.await.unwrap_or_default()
}

/// Deletes the jar `file` from the instance's mods folder, and its catalog
/// entry. `file` must be a plain `.jar` file name. A jar that's already gone
/// is fine.
pub async fn remove(inst: &Instance, file: &str) -> Result<()> {
    if !is_jar_file_name(file) {
        return Err(Error::NotAJar(file.to_owned()));
    }
    let dir = mods_dir(inst);
    let (work_dir, file) = (dir.clone(), file.to_owned());
    blocking(move || {
        remove_if_present(&work_dir.join(&file))?;
        let mut catalog = Catalog::read_for_update(&work_dir)?;
        if catalog.forget(&file) {
            catalog.write(&work_dir)?;
        }
        Ok(())
    })
    .await
    .map_err(Error::io(&dir))
}
