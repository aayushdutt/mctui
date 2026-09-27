//! Getting a plan's files in place: downloads, natives, and old versions'
//! asset layouts.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::plan::{Assets, Plan, join_relative};
use super::{Error, Result, Step, blocking, natives};
use crate::net::{Download, Hash, Http, Progress};

/// Where asset objects are downloaded from.
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

/// Downloads at a time: a few big jars, or many small assets.
const FILE_DOWNLOADS: usize = 8;
const ASSET_DOWNLOADS: usize = 16;

/// What [`prepare`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prepared {
    /// Every file was downloaded or checked against its checksum, so the
    /// instance can be marked fully downloaded under [`Plan::download_key`].
    /// `false` when it already was, and its files were only checked to be
    /// there.
    pub verified: bool,
}

/// Gets everything `plan` needs in place: downloads the game, libraries,
/// natives jars, logging setup, and assets (checksums verified, each file
/// once however often it's listed), unpacks the natives, lays out assets by
/// name for versions before 1.7.3, and creates the working folder.
///
/// An instance already fully downloaded ([`Plan::downloaded`]) skips the
/// network: its files are only checked to be there at the right size, and
/// if any is missing everything is fetched and verified after all (the Go
/// app trusted the mark blindly: L-11).
///
/// Reports [`Step::Libraries`], [`Step::Assets`], then [`Step::Natives`].
/// Dropping the future stops the downloads and leaves no partial file.
pub async fn prepare(http: &Http, plan: &Plan, on_step: impl Fn(Step) + Sync) -> Result<Prepared> {
    let checked = if plan.downloaded {
        all_there(plan).await?
    } else {
        None
    };
    let verified = checked.is_none();
    let index = match checked {
        Some(index) => {
            report_done(plan, &index, &on_step);
            index
        }
        None => fetch(http, plan, &on_step).await?,
    };
    if let Some(assets) = &plan.assets {
        lay_out(assets, index).await?;
    }
    on_step(Step::Natives);
    natives::unpack(plan).await?;
    let dir = &plan.working_dir;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(Error::io(dir))?;
    Ok(Prepared { verified })
}

/// Downloads the plan's files, then its asset index and objects, and
/// returns the index.
async fn fetch(http: &Http, plan: &Plan, on_step: &(impl Fn(Step) + Sync)) -> Result<AssetIndex> {
    let mut files = Vec::with_capacity(plan.files.len());
    for file in &plan.files {
        // A file with no published checksum (a few loader libraries) is
        // kept once it's there: its versioned path never changes, and
        // fetching it every time would stop the instance launching offline.
        if file.hash.is_none() && tokio::fs::try_exists(&file.dest).await.unwrap_or(false) {
            continue;
        }
        files.push(file.clone());
    }
    download(http, files, FILE_DOWNLOADS, |p| on_step(Step::Libraries(p))).await?;

    let Some(assets) = &plan.assets else {
        return Ok(AssetIndex::default());
    };
    on_step(Step::Assets(Progress::default()));
    http.download(&assets.index, |_| {}).await?;
    let path = &assets.index.dest;
    let bytes = tokio::fs::read(path).await.map_err(Error::io(path))?;
    let index = AssetIndex::parse(path, &bytes)?;
    let objects = index
        .objects
        .values()
        .map(|object| assets.object_download(object))
        .collect();
    download(http, objects, ASSET_DOWNLOADS, |p| on_step(Step::Assets(p))).await?;
    Ok(index)
}

/// Downloads `files`, each destination once (an asset index lists some
/// objects under several names: L-5), reporting the totals first.
async fn download(
    http: &Http,
    mut files: Vec<Download>,
    concurrency: usize,
    report: impl Fn(Progress) + Sync,
) -> Result<()> {
    files.sort_by(|a, b| a.dest.cmp(&b.dest));
    files.dedup_by(|a, b| a.dest == b.dest);
    report(Progress {
        files_total: files.len(),
        bytes_total: files.iter().filter_map(|f| f.size).sum(),
        ..Progress::default()
    });
    http.download_all(files, concurrency, report).await?;
    Ok(())
}

/// The asset index, if every file of `plan` is on disk at its expected
/// size; `None` if something's missing.
async fn all_there(plan: &Plan) -> Result<Option<AssetIndex>> {
    let plan = plan.clone();
    let found = blocking(move || Ok(find_missing(&plan))).await?;
    match found {
        Ok(index) => Ok(Some(index)),
        Err(missing) => {
            tracing::info!(missing = %missing.display(), "a game file is missing; downloading again");
            Ok(None)
        }
    }
}

/// The asset index, if every file is there; else the first one missing.
/// Blocking.
fn find_missing(plan: &Plan) -> Result<AssetIndex, PathBuf> {
    let there = |path: &Path, size: Option<u64>| {
        fs::metadata(path).is_ok_and(|m| m.is_file() && size.is_none_or(|size| m.len() == size))
    };
    if let Some(file) = plan.files.iter().find(|f| !there(&f.dest, f.size)) {
        return Err(file.dest.clone());
    }
    let Some(assets) = &plan.assets else {
        return Ok(AssetIndex::default());
    };
    let path = &assets.index.dest;
    let index = fs::read(path)
        .ok()
        .and_then(|bytes| AssetIndex::parse(path, &bytes).ok())
        .ok_or_else(|| path.clone())?;
    for object in index.objects.values() {
        let path = assets.object_path(&object.hash);
        if !there(&path, Some(object.size)) {
            return Err(path);
        }
    }
    Ok(index)
}

/// Reports the file steps done, for an instance whose files were all there.
fn report_done(plan: &Plan, index: &AssetIndex, on_step: &impl Fn(Step)) {
    let done = |files: usize, bytes: u64| Progress {
        files_done: files,
        files_total: files,
        bytes_done: bytes,
        bytes_total: bytes,
    };
    let bytes = plan.files.iter().filter_map(|f| f.size).sum();
    on_step(Step::Libraries(done(plan.files.len(), bytes)));
    if plan.assets.is_some() {
        let bytes = index.objects.values().map(|o| o.size).sum();
        on_step(Step::Assets(done(index.objects.len(), bytes)));
    }
}

/// Copies assets by name where versions before 1.7.3 look for them, if the
/// plan's layout says to. Files already there at the right size are kept.
async fn lay_out(assets: &Assets, index: AssetIndex) -> Result<()> {
    let Some(dir) = assets.layout.named_dir() else {
        return Ok(());
    };
    let (dir, assets) = (dir.to_owned(), assets.clone());
    blocking(move || {
        for (name, object) in &index.objects {
            let dest = join_relative(&dir, name).ok_or_else(|| Error::UnsafePath(name.clone()))?;
            if fs::metadata(&dest).is_ok_and(|m| m.len() == object.size) {
                continue;
            }
            copy_into_place(&assets.object_path(&object.hash), &dest)?;
        }
        Ok(())
    })
    .await
}

/// Copies `from` to `to` through a temp file beside it, so `to` is never
/// half-written. Blocking.
fn copy_into_place(from: &Path, to: &Path) -> Result<()> {
    let dir = to.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(Error::io(dir))?;
    let temp = tempfile::NamedTempFile::new_in(dir).map_err(Error::io(dir))?;
    fs::copy(from, temp.path()).map_err(Error::io(from))?;
    temp.persist(to).map_err(|e| Error::io(to)(e.error))?;
    Ok(())
}

/// An asset index: every asset object by name.
#[derive(Debug, Default, Deserialize)]
struct AssetIndex {
    #[serde(default)]
    objects: BTreeMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    /// SHA-1, which also names the file.
    hash: String,
    size: u64,
}

impl AssetIndex {
    /// Decodes the index at `path`, making sure each hash is a SHA-1 (it
    /// becomes a file name).
    fn parse(path: &Path, bytes: &[u8]) -> Result<Self> {
        let damaged = |source| Error::AssetIndex {
            path: path.to_owned(),
            source,
        };
        let index: Self = serde_json::from_slice(bytes).map_err(damaged)?;
        let bad = index.objects.iter().find(|(_, o)| !is_sha1(&o.hash));
        if let Some((name, _)) = bad {
            let message = format!("{name:?} has no proper hash");
            return Err(damaged(serde::de::Error::custom(message)));
        }
        Ok(index)
    }
}

fn is_sha1(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Assets {
    /// Where an object is kept: `objects/<first two of hash>/<hash>`.
    fn object_path(&self, hash: &str) -> PathBuf {
        self.objects_dir.join(&hash[..2]).join(hash)
    }

    fn object_download(&self, object: &AssetObject) -> Download {
        let hash = &object.hash;
        Download {
            url: format!(
                "{}/{}/{hash}",
                self.objects_url.trim_end_matches('/'),
                &hash[..2]
            ),
            dest: self.object_path(hash),
            hash: Some(Hash::Sha1(hash.clone())),
            size: Some(object.size),
        }
    }
}
