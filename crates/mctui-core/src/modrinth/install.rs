//! Installing a mod, with its required dependencies, into an instance.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read as _};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use futures_util::{StreamExt as _, TryStreamExt as _, stream};

use super::catalog::{Catalog, Entry};
use super::client::{Client, found};
use super::model::{Project, Version, choose_version};
use super::resolve::{Answers, Lookup, Pick, Plan, SkippedDep, Target, UnresolvedDep, resolve};
use super::{Error, Result, blocking, mods_dir, remove_if_present};
use crate::facets;
use crate::instance::{Instance, Loader};
use crate::net::{Download, Hash, Progress};

/// How many requests, or downloads, run at once.
const CONCURRENCY: usize = 4;

/// A mod an install added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedMod {
    pub project_id: String,
    /// Empty if Modrinth didn't say.
    pub slug: String,
    pub title: String,
    /// The version string ("13.0.4").
    pub version_number: String,
    /// The jar's file name in the mods folder.
    pub file: String,
    /// Whether it came in as a dependency of the requested mod.
    pub dependency: bool,
}

/// What [`Client::install`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallReport {
    /// The requested mod first, then the dependencies it needed.
    pub installed: Vec<AddedMod>,
    /// Older jars of the requested mod that were removed: it was an upgrade.
    pub replaced: Vec<String>,
    /// Dependencies deliberately not installed.
    pub skipped: Vec<SkippedDep>,
    /// Required dependencies that couldn't be installed; worth a warning.
    pub unresolved: Vec<UnresolvedDep>,
}

impl Client {
    /// Installs `project` (an id or slug) into the instance: its newest
    /// version for the instance's Minecraft version and loader (see
    /// [`choose_version`]) and every required dependency not installed yet.
    /// A project that's installed already is upgraded, its old jar replaced.
    ///
    /// `on_progress` follows the downloads. Dependencies are downloaded before
    /// the requested mod, so it only lands once everything it needs is in
    /// place, and the catalog is written only after that. Dropping the future
    /// cancels the install: a dependency may have landed, but never the
    /// requested mod beside its old jar.
    pub async fn install(
        &self,
        inst: &Instance,
        project: &str,
        on_progress: impl Fn(Progress) + Sync,
    ) -> Result<InstallReport> {
        let target = target(inst)?;
        let root = Lookup::Project(project.to_owned());
        let (local, root) = tokio::try_join!(self.local_mods(inst), self.lookup(&root, target))?;
        let root = root
            .and_then(|version| {
                Some(Pick {
                    jar: target.jar(&version)?.clone(),
                    version,
                })
            })
            .ok_or_else(|| Error::NoCompatibleVersion {
                project: project.to_owned(),
                game_version: target.game_version.to_owned(),
                loader: target.loader.to_owned(),
            })?;
        let plan = self.plan(&root, target, &local.project_ids()).await?;
        let projects = self.projects_of(&plan, &local).await?;

        let dir = mods_dir(inst);
        let jobs = plan
            .picks
            .iter()
            .map(|pick| Download {
                url: pick.jar.url.clone(),
                dest: dir.join(&pick.jar.filename),
                hash: Some(Hash::Sha1(pick.jar.hashes.sha1.clone())),
                size: Some(pick.jar.size),
            })
            .collect();
        let report = report(plan, local.replaced_by(&root), &projects);
        let record = catalog_entries(&report, &local, &projects);

        self.download(jobs, on_progress).await?;
        // Nothing is awaited between the requested mod landing and the commit
        // starting, and once started the commit runs to the end.
        let (commit_dir, replaced) = (dir.clone(), report.replaced.clone());
        blocking(move || commit(&commit_dir, &replaced, record))
            .await
            .map_err(Error::io(&dir))?;
        Ok(report)
    }

    /// Walks the dependencies of `root`, fetching each level's lookups
    /// together.
    async fn plan(
        &self,
        root: &Pick,
        target: Target<'_>,
        installed: &HashSet<String>,
    ) -> Result<Plan> {
        let mut answers = Answers::new();
        loop {
            match resolve(root, target, &answers, installed) {
                Ok(plan) => return Ok(plan),
                Err(missing) => {
                    let fetched: Vec<_> = stream::iter(missing)
                        .map(|lookup| async move {
                            let answer = self.lookup(&lookup, target).await?;
                            Ok::<_, Error>((lookup, answer))
                        })
                        .buffer_unordered(CONCURRENCY)
                        .try_collect()
                        .await?;
                    answers.extend(fetched);
                }
            }
        }
    }

    /// Answers a [`Lookup`]: `None` when there's no such version, or no
    /// version of the project that fits.
    async fn lookup(&self, lookup: &Lookup, target: Target<'_>) -> Result<Option<Version>> {
        match lookup {
            Lookup::Project(id) => {
                let versions = self
                    .versions(id, &[target.loader], &[target.game_version])
                    .await;
                let versions = found(versions)?.unwrap_or_default();
                Ok(choose_version(&versions, target.game_version, target.loader).cloned())
            }
            Lookup::Version(id) => found(self.version(id).await),
        }
    }

    /// The projects that a plan and the newly identified jars name, by id,
    /// for their slugs and titles.
    async fn projects_of(
        &self,
        plan: &Plan,
        local: &LocalMods,
    ) -> Result<HashMap<String, Project>> {
        let picked = plan
            .picks
            .iter()
            .map(|pick| pick.version.project_id.as_str());
        let skipped = plan.skipped.iter().filter_map(|d| d.project_id.as_deref());
        let unresolved = plan
            .unresolved
            .iter()
            .filter_map(|d| d.project_id.as_deref());
        let identified = local.identified.iter().map(|(id, _)| id.as_str());
        let mut ids: Vec<&str> = picked
            .chain(skipped)
            .chain(unresolved)
            .chain(identified)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let projects = self.projects(&ids).await?;
        Ok(projects.into_iter().map(|p| (p.id.clone(), p)).collect())
    }

    /// Which projects the instance has: from the catalog, and from Modrinth by
    /// SHA-1 for jars the catalog doesn't vouch for.
    async fn local_mods(&self, inst: &Instance) -> Result<LocalMods> {
        let dir = mods_dir(inst);
        let scan_inst = inst.clone();
        let (mut local, unknown) = blocking(move || Ok(LocalMods::scan(&scan_inst)))
            .await
            .map_err(Error::io(&dir))?;
        let identified: Vec<_> = stream::iter(unknown)
            .map(|(file, sha1)| async move {
                Ok::<_, Error>((file, self.version_from_sha1(&sha1).await?))
            })
            .buffer_unordered(CONCURRENCY)
            .try_collect()
            .await?;
        for (file, version) in identified {
            if let Some(version) = version {
                local.add(version.project_id.clone(), file.clone());
                local.identified.push((version.project_id, file));
            }
        }
        Ok(local)
    }

    /// Downloads the dependencies, then the requested mod (`jobs[0]`), with
    /// one running progress. (`Http::download_all` would do, but the compiler
    /// can't prove its future `Send`, and installs must be spawnable.)
    async fn download(
        &self,
        jobs: Vec<Download>,
        on_progress: impl Fn(Progress) + Sync,
    ) -> Result<()> {
        let progress = Mutex::new(Progress {
            files_total: jobs.len(),
            bytes_total: jobs.iter().filter_map(|j| j.size).sum(),
            ..Progress::default()
        });
        let advance = |files: usize, bytes: u64| {
            let now = {
                let mut p = progress.lock().unwrap_or_else(PoisonError::into_inner);
                p.files_done += files;
                p.bytes_done += bytes;
                *p
            };
            on_progress(now);
        };
        let fetch = |job: Download| async move {
            self.http.download(&job, |n| advance(0, n)).await?;
            advance(1, 0);
            Ok::<_, Error>(())
        };
        let mut jobs = jobs.into_iter();
        let root = jobs.next();
        stream::iter(jobs)
            .map(fetch)
            .buffer_unordered(CONCURRENCY)
            .try_collect::<()>()
            .await?;
        if let Some(root) = root {
            fetch(root).await?;
        }
        Ok(())
    }
}

/// The jars already in an instance's mods folder.
#[derive(Debug, Default)]
struct LocalMods {
    /// Project id → its jars.
    jars: HashMap<String, Vec<String>>,
    /// Jars identified by SHA-1, as (project id, file), to record in the
    /// catalog: ones installed by hand, or recorded by slug by the Go app.
    identified: Vec<(String, String)>,
}

impl LocalMods {
    /// Lists the jars the catalog vouches for, and the SHA-1s of the others.
    fn scan(inst: &Instance) -> (Self, Vec<(String, String)>) {
        let dir = mods_dir(inst);
        let catalog = Catalog::read(&dir);
        let mut local = Self::default();
        let mut unknown = Vec::new();
        for jar in facets::mod_jars(inst) {
            let file = jar.file_name;
            match catalog.entry_for(&file) {
                Some(entry) if entry.has_real_id() => local.add(entry.project_id.clone(), file),
                _ => match sha1_of(&dir.join(&file)) {
                    Ok(sha1) => unknown.push((file, sha1)),
                    Err(e) => tracing::warn!(file, error = %e, "couldn't read mod jar"),
                },
            }
        }
        (local, unknown)
    }

    fn add(&mut self, project_id: String, file: String) {
        self.jars.entry(project_id).or_default().push(file);
    }

    fn project_ids(&self) -> HashSet<String> {
        self.jars.keys().cloned().collect()
    }

    /// The jars of `root`'s project that installing it replaces.
    fn replaced_by(&self, root: &Pick) -> Vec<String> {
        let jars = self.jars.get(&root.version.project_id);
        jars.into_iter()
            .flatten()
            .filter(|file| !file.eq_ignore_ascii_case(&root.jar.filename))
            .cloned()
            .collect()
    }
}

/// The report for `plan`, with names from `projects`.
fn report(plan: Plan, replaced: Vec<String>, projects: &HashMap<String, Project>) -> InstallReport {
    let title_of = |id: &Option<String>, name: String| {
        id.as_ref()
            .and_then(|id| projects.get(id))
            .map_or(name, |p| p.title.clone())
    };
    InstallReport {
        installed: plan
            .picks
            .into_iter()
            .enumerate()
            .map(|(i, pick)| {
                let project = projects.get(&pick.version.project_id);
                AddedMod {
                    slug: project.map(|p| p.slug.clone()).unwrap_or_default(),
                    title: project.map_or_else(
                        || facets::parse_mod_file_name(&pick.jar.filename).0,
                        |p| p.title.clone(),
                    ),
                    project_id: pick.version.project_id,
                    version_number: pick.version.version_number,
                    file: pick.jar.filename,
                    dependency: i > 0,
                }
            })
            .collect(),
        replaced,
        skipped: plan
            .skipped
            .into_iter()
            .map(|d| SkippedDep {
                name: title_of(&d.project_id, d.name),
                ..d
            })
            .collect(),
        unresolved: plan
            .unresolved
            .into_iter()
            .map(|d| UnresolvedDep {
                name: title_of(&d.project_id, d.name),
                ..d
            })
            .collect(),
    }
}

/// The catalog entries an install records: the jars it identified (but not
/// ones it replaces), then the ones it added.
fn catalog_entries(
    report: &InstallReport,
    local: &LocalMods,
    projects: &HashMap<String, Project>,
) -> Vec<Entry> {
    let entry = |project_id: &str, file: &str| {
        let project = projects.get(project_id);
        Entry {
            project_id: project_id.to_owned(),
            slug: project.map(|p| p.slug.clone()).unwrap_or_default(),
            file: file.to_owned(),
            title: project.map(|p| p.title.clone()),
        }
    };
    let identified = local
        .identified
        .iter()
        .filter(|(_, file)| !report.replaced.contains(file))
        .map(|(id, file)| entry(id, file));
    let added = report
        .installed
        .iter()
        .map(|m| entry(&m.project_id, &m.file));
    identified.chain(added).collect()
}

/// Removes the `replaced` jars and records the new ones, as one blocking task
/// so that once started it finishes even if the install is cancelled.
fn commit(dir: &Path, replaced: &[String], record: Vec<Entry>) -> io::Result<()> {
    for file in replaced {
        remove_if_present(&dir.join(file))?;
    }
    let mut catalog = Catalog::read_for_update(dir)?;
    for file in replaced {
        catalog.forget(file);
    }
    for entry in record {
        catalog.record(entry);
    }
    catalog.write(dir)
}

/// What the instance's mods must support.
fn target(inst: &Instance) -> Result<Target<'_>> {
    if inst.loader == Loader::Vanilla {
        return Err(Error::NoModLoader);
    }
    Ok(Target {
        game_version: &inst.version,
        loader: inst.loader.as_str(),
    })
}

/// The SHA-1 of the file at `path`, as lowercase hex.
fn sha1_of(path: &Path) -> io::Result<String> {
    use sha1::Digest as _;
    use std::fmt::Write as _;

    let mut file = fs::File::open(path)?;
    let mut hasher = sha1::Sha1::default();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_of_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.jar");
        fs::write(&path, "hello").unwrap();
        assert_eq!(
            sha1_of(&path).unwrap(),
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"
        );
    }
}
