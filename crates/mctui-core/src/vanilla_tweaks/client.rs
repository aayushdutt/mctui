//! The site's endpoints, the offline catalog cache, and putting a build into
//! an instance.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{BASE_URL, Cart, Catalog, CatalogVersion, Error, Pack, options};
use crate::net::{self, Download, Http};
use crate::paths::DataDir;
use crate::{Loaded, fsutil};

/// Talks to Vanilla Tweaks. Cheap to clone.
#[derive(Debug, Clone)]
pub struct VanillaTweaks {
    http: Http,
    /// The site root, without a trailing slash.
    base: String,
    /// Where catalogs are kept for offline browsing.
    cache_dir: PathBuf,
}

/// The build endpoint's answer. Its status is in the body, whatever the HTTP
/// status: 200 with a link, or 400 with a message for packs it doesn't know.
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum BuildResponse {
    Success {
        link: String,
    },
    Error {
        #[serde(default)]
        message: String,
    },
}

impl VanillaTweaks {
    /// A client for the live site that keeps catalogs in the data dir's cache
    /// folder.
    pub fn new(http: Http, data_dir: &DataDir) -> Self {
        Self {
            http,
            base: BASE_URL.to_owned(),
            cache_dir: data_dir.cache_dir().join("vanillatweaks"),
        }
    }

    /// Talks to `base` instead of the live site (for tests).
    #[must_use]
    pub fn with_base_url(self, base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            ..self
        }
    }

    /// The catalog for `version`. Each fetch is kept for offline use: when the
    /// site can't be reached or answers badly, the kept copy is served with a
    /// notice saying so. Fails only when there's no kept copy either.
    pub async fn catalog(&self, version: CatalogVersion) -> Result<Loaded<Catalog>, Error> {
        let kept = self.cache_dir.join(format!("rpcategories-{version}.json"));
        let err = match self.fetch_catalog(version).await {
            Ok((catalog, json)) => {
                keep(kept, json).await;
                return Ok(Loaded::clean(catalog));
            }
            Err(err) => err,
        };
        let Some(catalog) = read_kept(&kept, version).await else {
            return Err(err.into());
        };
        let notice = format!(
            "Couldn't get the latest packs from Vanilla Tweaks ({err}) — \
             showing the ones from your last visit."
        );
        Ok(Loaded {
            value: catalog,
            notice: Some(notice),
        })
    }

    /// A link to the pack's preview on the site (animated when there is one),
    /// for opening in a browser.
    pub fn preview_url(&self, version: CatalogVersion, pack: &Pack) -> String {
        let extension = pack.preview_extension.as_deref().unwrap_or("png");
        format!(
            "{}/assets/resources/previews/resourcepacks/{version}/{}.{extension}",
            self.base, pack.name
        )
    }

    /// Makes the instance match the cart: has the site merge the picked packs
    /// for `version` into one zip, downloads it to
    /// `resourcepacks/VanillaTweaks.zip`, enables it in `options.txt`, then
    /// marks the cart applied and saves it. An empty cart removes the pack
    /// instead, as [`unapply`] does.
    ///
    /// Nothing is touched until the new zip has fully downloaded (it lands
    /// beside the old one and is renamed over it), so a failed build or
    /// download, or cancelling by dropping the future, leaves the old pack,
    /// `options.txt`, and the cart as they were.
    pub async fn apply(
        &self,
        game_dir: &Path,
        version: CatalogVersion,
        cart: &mut Cart,
    ) -> Result<(), Error> {
        if cart.is_empty() {
            return unapply(game_dir, cart).await;
        }
        let url = self.build(version, cart).await?;
        let zip = Download {
            url,
            dest: super::pack_path(game_dir),
            hash: None,
            size: None,
        };
        self.http.download(&zip, |_| {}).await?;

        options::enable(game_dir).await?;
        let mut applied = cart.clone();
        applied.mark_applied(version);
        applied.save(game_dir).await?;
        *cart = applied;
        Ok(())
    }

    /// Fetches and decodes a catalog, returning the raw JSON too for the
    /// cache.
    async fn fetch_catalog(
        &self,
        version: CatalogVersion,
    ) -> Result<(Catalog, bytes::Bytes), net::Error> {
        let url = format!(
            "{}/assets/resources/json/{version}/rpcategories.json",
            self.base
        );
        let json = self.http.get_bytes(&url).await?;
        let catalog = Catalog::from_json(version, &json)
            .map_err(|source| net::Error::Decode { url, source })?;
        Ok((catalog, json))
    }

    /// Asks the site to merge the picked packs into one zip, returning its
    /// download URL. Not retried: the user can build again.
    async fn build(&self, version: CatalogVersion, cart: &Cart) -> Result<String, Error> {
        let url = format!("{}/assets/server/zipresourcepacks.php", self.base);
        let failed = |source| net::Error::Request {
            url: url.clone(),
            source,
        };
        let form = [
            ("packs", cart.picked().to_json()),
            ("version", version.to_string()),
        ];
        let response = self
            .http
            .client()
            .post(&url)
            .form(&form)
            .send()
            .await
            .map_err(failed)?;
        let status = response.status();
        let body = response.bytes().await.map_err(failed)?;
        match serde_json::from_slice(&body) {
            Ok(BuildResponse::Success { link }) => self.download_url(&link),
            Ok(BuildResponse::Error { message }) => Err(Error::Build(message)),
            Err(_) if !status.is_success() => Err(net::Error::Status { url, status }.into()),
            Err(source) => Err(net::Error::Decode { url, source }.into()),
        }
    }

    /// The build's download `link` (a path on the site) as a URL. A link to
    /// anywhere but the site is refused, not followed.
    fn download_url(&self, link: &str) -> Result<String, Error> {
        let bad_link = || Error::BadLink(link.to_owned());
        let base = reqwest::Url::parse(&self.base).map_err(|_| bad_link())?;
        let url = base.join(link).map_err(|_| bad_link())?;
        if url.origin() != base.origin() {
            return Err(bad_link());
        }
        Ok(url.into())
    }
}

/// Removes the merged pack from the instance: disables it in `options.txt`,
/// deletes the zip, and marks the cart unapplied. The picks stay in the cart
/// for a later apply.
pub async fn unapply(game_dir: &Path, cart: &mut Cart) -> Result<(), Error> {
    options::disable(game_dir).await?;
    let zip = super::pack_path(game_dir);
    if let Err(source) = tokio::fs::remove_file(&zip).await
        && source.kind() != io::ErrorKind::NotFound
    {
        return Err(Error::Io { path: zip, source });
    }
    let mut unapplied = cart.clone();
    unapplied.mark_unapplied();
    unapplied.save(game_dir).await?;
    *cart = unapplied;
    Ok(())
}

/// Whether the merged pack is in the instance and enabled in `options.txt`.
/// False once the player turns it off in Minecraft or deletes it, whatever
/// the cart says.
pub async fn is_enabled(game_dir: &Path) -> bool {
    let installed = tokio::fs::try_exists(super::pack_path(game_dir))
        .await
        .unwrap_or(false);
    installed && options::is_enabled(game_dir).await
}

/// Keeps a fetched catalog at `path` for offline use. Failing to is logged,
/// not reported: the catalog itself arrived fine.
async fn keep(path: PathBuf, json: bytes::Bytes) {
    let saved = super::blocking(path, move |path| {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fsutil::write_atomic(path, &json, 0o644)
    });
    if let Err(e) = saved.await {
        tracing::warn!(error = %e, "couldn't keep the catalog for offline use");
    }
}

/// The catalog kept at `path`, if there's a readable one.
async fn read_kept(path: &Path, version: CatalogVersion) -> Option<Catalog> {
    let json = tokio::fs::read(path).await.ok()?;
    Catalog::from_json(version, &json).ok()
}
