//! The on-disk cache of fetched metadata, kept byte for byte as served so
//! nothing is lost in a round trip, and read back for offline use.

use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::fsutil;
use crate::net::{self, Http};
use crate::paths::DataDir;

/// Where version JSONs and loader profiles are cached: `<data>/cache/versions`,
/// as the Go app had it.
pub(crate) fn versions_dir(data: &DataDir) -> PathBuf {
    data.cache_dir().join("versions")
}

/// A cached file that couldn't be used.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// The file is there but couldn't be read.
    #[error("couldn't read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The file isn't the JSON it should be.
    #[error("unexpected contents in {}: {source}", path.display())]
    Malformed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// `<dir>/<name>.json`, or `None` when `name` would reach outside `dir`.
pub(crate) fn file(dir: &Path, name: &str) -> Option<PathBuf> {
    let safe = !name.is_empty() && !name.contains(['/', '\\', '\0']);
    safe.then(|| dir.join(format!("{name}.json")))
}

/// GETs `url` and decodes it, keeping the raw bytes to [`save`].
pub(crate) async fn fetch<T: DeserializeOwned>(
    http: &Http,
    url: &str,
) -> net::Result<(T, bytes::Bytes)> {
    let raw = http.get_bytes(url).await?;
    let value = serde_json::from_slice(&raw).map_err(|source| net::Error::Decode {
        url: url.to_owned(),
        source,
    })?;
    Ok((value, raw))
}

/// Reads and decodes a cached file; `Ok(None)` when there isn't one.
pub(crate) async fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, CacheError> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(CacheError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|source| CacheError::Malformed {
            path: path.to_owned(),
            source,
        })
}

/// The cached copy to use when fetching failed, if there's a usable one.
pub(crate) async fn fallback<T: DeserializeOwned>(path: &Path) -> Option<T> {
    read(path).await.unwrap_or_else(|error| {
        tracing::warn!(%error, "the cached copy is unusable");
        None
    })
}

/// Saves `bytes` as `path`, atomically. Best effort: what was fetched is good
/// either way, so a failure is only logged. Finishes even if the caller's
/// future is dropped.
pub(crate) async fn save(path: PathBuf, bytes: bytes::Bytes) {
    let write = tokio::task::spawn_blocking(move || {
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| fsutil::write_atomic(&path, &bytes, 0o644));
        if let Err(error) = result {
            tracing::warn!(path = %path.display(), %error, "couldn't cache");
        }
    });
    if let Err(error) = write.await {
        tracing::warn!(%error, "caching failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_stay_inside_the_dir() {
        let dir = Path::new("/cache/versions");
        assert_eq!(
            file(dir, "1.14 Pre-Release 5"),
            Some(dir.join("1.14 Pre-Release 5.json"))
        );
        assert_eq!(file(dir, ".."), Some(dir.join("...json")));
        for bad in ["", "../escape", "a/b", "a\\b", "nul\0"] {
            assert_eq!(file(dir, bad), None, "{bad:?}");
        }
    }

    #[tokio::test]
    async fn save_then_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/v.json");
        assert!(read::<Vec<u32>>(&path).await.unwrap().is_none());

        save(path.clone(), bytes::Bytes::from_static(b"[1, 2]")).await;
        assert_eq!(read::<Vec<u32>>(&path).await.unwrap(), Some(vec![1, 2]));

        std::fs::write(&path, "{").unwrap();
        let err = read::<Vec<u32>>(&path).await.unwrap_err();
        assert!(matches!(err, CacheError::Malformed { .. }), "{err}");
    }
}
