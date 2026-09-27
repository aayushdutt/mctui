//! The starter mods offered for new Fabric instances.

use super::install::InstallReport;
use super::{Client, Result};
use crate::instance::Instance;
use crate::net::Progress;

/// A mod in the starter set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterMod {
    pub slug: &'static str,
    pub title: &'static str,
}

/// The mods offered for new Fabric instances, in install order: Fabric API
/// first, since the others build on it.
pub const STARTER_MODS: [StarterMod; 4] = [
    StarterMod {
        slug: "fabric-api",
        title: "Fabric API",
    },
    StarterMod {
        slug: "modmenu",
        title: "Mod Menu",
    },
    StarterMod {
        slug: "sodium",
        title: "Sodium",
    },
    StarterMod {
        slug: "lithium",
        title: "Lithium",
    },
];

/// Progress of [`Client::install_starter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterProgress {
    /// The mod being installed, and its place in [`STARTER_MODS`].
    pub starter: StarterMod,
    pub index: usize,
    /// Its download, dependencies included.
    pub download: Progress,
}

/// How installing one starter mod went.
#[derive(Debug)]
pub struct StarterOutcome {
    pub starter: StarterMod,
    pub result: Result<InstallReport>,
}

/// What [`Client::install_starter`] did, mod by mod.
#[derive(Debug)]
pub struct StarterReport {
    pub mods: Vec<StarterOutcome>,
}

impl StarterReport {
    /// Whether every starter mod was installed with all it needs, so the
    /// instance's `install_starter_fabric_mods` can be cleared. Otherwise a
    /// later try may do better: the network is back, or a mod has caught up
    /// with a new Minecraft version.
    pub fn all_ok(&self) -> bool {
        self.mods
            .iter()
            .all(|m| m.result.as_ref().is_ok_and(|r| r.unresolved.is_empty()))
    }
}

impl Client {
    /// Installs [`STARTER_MODS`] one by one. A mod that fails (no version for
    /// this Minecraft yet, say) is reported, and the rest are still installed.
    /// `on_progress` hears of each mod as it starts, then of its download.
    pub async fn install_starter(
        &self,
        inst: &Instance,
        on_progress: impl Fn(StarterProgress) + Sync,
    ) -> StarterReport {
        let mut mods = Vec::with_capacity(STARTER_MODS.len());
        for (index, starter) in STARTER_MODS.into_iter().enumerate() {
            let report = |download| {
                on_progress(StarterProgress {
                    starter,
                    index,
                    download,
                });
            };
            report(Progress::default());
            let result = self.install(inst, starter.slug, &report).await;
            if let Err(e) = &result {
                tracing::warn!(starter = starter.slug, error = %e, "starter mod not installed");
            }
            mods.push(StarterOutcome { starter, result });
        }
        StarterReport { mods }
    }
}
