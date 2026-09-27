//! Vanilla Tweaks resource packs (<https://vanillatweaks.net>): the catalog of
//! packs for a Minecraft version, a per-instance cart of picked packs, and
//! applying the cart as one merged `VanillaTweaks.zip` enabled in
//! `options.txt`.
//!
//! The flow: [`CatalogVersion::for_minecraft`] picks an instance's catalog (or
//! says why there's none), [`VanillaTweaks::catalog`] loads it, the UI edits
//! the [`Cart`] from [`Cart::load`], and [`VanillaTweaks::apply`] builds the
//! picks into the instance; [`unapply`] takes them out again.

mod cart;
mod catalog;
mod client;
mod options;
mod version;

use std::io;
use std::path::{Path, PathBuf};

pub(crate) use cart::CART_FILE;
pub use cart::{Cart, PackRef, PackState, Toggled};
pub use catalog::{Catalog, Category, Pack, Warning, plain_text};
pub use client::{Stage, VanillaTweaks, is_enabled, unapply};
pub use version::{CatalogVersion, Unsupported};

use crate::net;

/// The live site.
pub const BASE_URL: &str = "https://vanillatweaks.net";

/// The merged pack's file name in `.minecraft/resourcepacks`.
pub const PACK_FILE: &str = "VanillaTweaks.zip";

/// Why loading, building, or applying packs failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The site couldn't be reached or answered badly.
    #[error(transparent)]
    Net(#[from] net::Error),
    /// The site wouldn't build the pack, and said why.
    #[error("Vanilla Tweaks couldn't build the pack: {0}")]
    Build(String),
    /// The build's download link pointed away from the site.
    #[error("Vanilla Tweaks sent an unexpected download link: {0}")]
    BadLink(String),
    /// A file in the instance or the cache couldn't be read or written.
    #[error("couldn't update {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Where the merged pack goes in an instance.
fn pack_path(game_dir: &Path) -> PathBuf {
    game_dir.join("resourcepacks").join(PACK_FILE)
}

/// Runs blocking file work on `path` off the async runtime, naming `path` in
/// any error.
async fn blocking<T, F>(path: PathBuf, work: F) -> Result<T, Error>
where
    T: Send + 'static,
    F: FnOnce(&Path) -> io::Result<T> + Send + 'static,
{
    let target = path.clone();
    tokio::task::spawn_blocking(move || work(&target))
        .await
        .unwrap_or_else(|join| Err(io::Error::other(join)))
        .map_err(|source| Error::Io { path, source })
}
