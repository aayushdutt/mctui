//! Mojang's version metadata: the manifest of every game version, each
//! version's JSON, and the rules and Maven coordinates inside it.
//!
//! Everything fetched is cached under `<data>/cache` exactly as served, so a
//! version that launched once launches again without Mojang: when fetching
//! fails, the cached copy is used and marked [`Fetched::stale`].

pub(crate) mod cache;
mod manifest;
pub mod maven;
pub mod rules;
mod version;

use std::path::PathBuf;

pub use cache::CacheError;
pub use manifest::{Latest, Manifest, VersionEntry, VersionKind};
pub use version::{
    Argument, Arguments, Artifact, AssetIndexRef, Downloads, Extract, JavaVersion, Library,
    LibraryDownloads, Logging, LoggingConfig, LoggingFile, RemoteFile, VersionJson,
};

use crate::net::{self, Download, Hash, Http};
use crate::paths::DataDir;

/// Mojang's version manifest.
pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Why version metadata couldn't be had.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Fetching the manifest failed and there's no cached copy.
    #[error("couldn't get the list of Minecraft versions: {0}")]
    Manifest(#[source] net::Error),
    /// Fetching a version's JSON failed and there's no cached copy.
    #[error("couldn't get Minecraft {id}: {source}")]
    Version {
        id: String,
        #[source]
        source: net::Error,
    },
    /// No such version (or an id that can't name a file).
    #[error("Minecraft {0} isn't in Mojang's list of versions")]
    UnknownVersion(String),
    /// Offline, and the version was never fetched.
    #[error("Minecraft {0} hasn't been downloaded yet; connect to the internet to set it up")]
    NotCached(String),
    /// A cached file exists but can't be used.
    #[error(transparent)]
    Cache(#[from] CacheError),
}

/// A result with this module's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A value from a service, or from the cache when the service couldn't be
/// reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched<T> {
    /// What was fetched, or cached.
    pub value: T,
    /// Taken from the cache after fetching failed, so it may be out of date.
    pub stale: bool,
}

impl<T> Fetched<T> {
    fn fresh(value: T) -> Self {
        Self {
            value,
            stale: false,
        }
    }

    fn stale(value: T) -> Self {
        Self { value, stale: true }
    }
}

/// Fetches version metadata from Mojang and caches it. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Mojang {
    http: Http,
    manifest_url: String,
    manifest_file: PathBuf,
    versions_dir: PathBuf,
}

impl Mojang {
    /// A client for Mojang's servers, caching under `data`'s cache dir.
    pub fn new(http: Http, data: &DataDir) -> Self {
        Self::with_manifest_url(http, data, MANIFEST_URL)
    }

    /// A client that reads the manifest from `manifest_url`. Version JSONs
    /// come from wherever the manifest says.
    pub fn with_manifest_url(http: Http, data: &DataDir, manifest_url: impl Into<String>) -> Self {
        Self {
            http,
            manifest_url: manifest_url.into(),
            manifest_file: data.cache_dir().join("version_manifest_v2.json"),
            versions_dir: cache::versions_dir(data),
        }
    }

    /// The manifest of every game version. Each fetch refreshes the cached
    /// copy; when fetching fails, the cached copy is returned instead.
    pub async fn manifest(&self) -> Result<Fetched<Manifest>> {
        match cache::fetch(&self.http, &self.manifest_url).await {
            Ok((manifest, raw)) => {
                cache::save(self.manifest_file.clone(), raw).await;
                Ok(Fetched::fresh(manifest))
            }
            Err(error) => {
                tracing::warn!(%error, "couldn't fetch the version manifest; trying the cache");
                let cached = cache::fallback(&self.manifest_file).await;
                cached.map(Fetched::stale).ok_or(Error::Manifest(error))
            }
        }
    }

    /// The JSON for `entry`, checked against the manifest's SHA-1 and cached
    /// as `cache/versions/<id>.json`. A cached copy with that checksum is
    /// used without going online; when the download fails, whatever copy is
    /// cached is returned instead.
    pub async fn version(&self, entry: &VersionEntry) -> Result<Fetched<VersionJson>> {
        let path = self.version_file(&entry.id)?;
        let job = Download {
            url: entry.url.clone(),
            dest: path.clone(),
            hash: Some(Hash::Sha1(entry.sha1.clone())),
            size: None,
        };
        // Downloads into the cache, or finds the cached copy already current.
        if let Err(error) = self.http.download(&job, |_| {}).await {
            tracing::warn!(id = entry.id, %error, "couldn't fetch the version JSON; trying the cache");
            let cached = cache::fallback(&path).await;
            return cached.map(Fetched::stale).ok_or_else(|| Error::Version {
                id: entry.id.clone(),
                source: error,
            });
        }
        self.cached_version(&entry.id).await.map(Fetched::fresh)
    }

    /// The cached JSON for version `id`, without going online.
    pub async fn cached_version(&self, id: &str) -> Result<VersionJson> {
        let path = self.version_file(id)?;
        cache::read(&path)
            .await?
            .ok_or_else(|| Error::NotCached(id.to_owned()))
    }

    /// The JSON for version `id`, however it can be had: through the
    /// manifest when Mojang is reachable, else from the cache (stale). This
    /// is what launching needs.
    pub async fn resolve(&self, id: &str) -> Result<Fetched<VersionJson>> {
        match self.manifest().await {
            Ok(manifest) => {
                let entry = manifest
                    .value
                    .find(id)
                    .ok_or_else(|| Error::UnknownVersion(id.to_owned()))?;
                let mut version = self.version(entry).await?;
                version.stale |= manifest.stale;
                Ok(version)
            }
            // Not even a cached manifest: the cached version is all there is.
            Err(error) => match self.cached_version(id).await {
                Ok(version) => Ok(Fetched::stale(version)),
                Err(Error::NotCached(_)) => Err(error),
                Err(other) => Err(other),
            },
        }
    }

    fn version_file(&self, id: &str) -> Result<PathBuf> {
        cache::file(&self.versions_dir, id).ok_or_else(|| Error::UnknownVersion(id.to_owned()))
    }
}
