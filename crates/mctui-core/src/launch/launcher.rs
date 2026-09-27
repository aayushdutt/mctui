//! The whole launch, from an instance to a running game.

use std::borrow::Cow;
use std::time::Duration;

use jiff::Timestamp;

use super::game::{DEFAULT_STOP_GRACE, Game, Logs};
use super::identity::Identity;
use super::plan::{PlanInputs, plan};
use super::prepare::{RESOURCES_URL, prepare};
use super::{Error, Result, Step};
use crate::config::Config;
use crate::fabric::{self, Fabric};
use crate::instance::{Instance, Loader};
use crate::java::{Choice, Pick, Runtime, Runtimes, StaleJava};
use crate::modrinth::{self, StarterReport};
use crate::mojang::rules::Env;
use crate::mojang::{Fetched, Mojang, VersionJson};
use crate::net::Http;
use crate::paths::DataDir;

/// Everything a launch talks to, and its settings. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Launcher {
    /// For downloads.
    pub http: Http,
    /// Game version metadata.
    pub mojang: Mojang,
    /// Fabric loader metadata.
    pub fabric: Fabric,
    /// Starter mods for new Fabric instances.
    pub modrinth: modrinth::Client,
    /// Where to find Java, and install it.
    pub runtimes: Runtimes,
    /// Where libraries and assets are kept.
    pub data: DataDir,
    /// Where asset objects come from: [`RESOURCES_URL`].
    pub resources_url: String,
    /// How long a stopped game gets before it's killed:
    /// [`DEFAULT_STOP_GRACE`].
    pub stop_grace: Duration,
}

/// A launched game, and what the launch learned.
#[derive(Debug)]
pub struct Launched {
    /// The running game.
    pub game: Game,
    /// Its output.
    pub logs: Logs,
    /// What the instance should remember; for the caller to save.
    pub changes: InstanceChanges,
    /// The Java it runs on. Found afresh each launch: an automatic pick
    /// isn't saved into the instance, where it would outlive the runtime
    /// and the version's needs (L-3).
    pub java: Runtime,
    /// The version's metadata came from the cache, as the services couldn't
    /// be reached; it may be out of date.
    pub stale: bool,
    /// How installing the starter mods went, if they were installed. A
    /// failure doesn't stop the launch (L-7); it may do better next time.
    pub starter_mods: Option<StarterReport>,
}

/// What a launch learned that the instance should remember. The launch
/// never saves the instance itself: [`InstanceChanges::apply`] this to the
/// app's copy and save it there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceChanges {
    /// The Fabric loader version picked for an instance that had none.
    pub loader_version: Option<String>,
    /// Every game file was verified for this [`Instance::download_key`], so
    /// the instance can launch offline.
    pub downloaded: Option<String>,
    /// Every starter mod is installed, so the instance needn't try again.
    pub starter_mods_installed: bool,
    /// The instance's own Java was set aside as stale (see [`StaleJava`]),
    /// so it should forget it. It says why, for a notice.
    pub stale_java: Option<StaleJava>,
}

impl InstanceChanges {
    /// Whether there's nothing to save.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Applies the changes to `instance`. The download mark only sticks if
    /// the instance still has the version and loader it was launched with,
    /// and a stale Java is only forgotten if the instance still has it.
    pub fn apply(&self, instance: &mut Instance) {
        if let Some(version) = &self.loader_version
            && instance.loader_version.trim().is_empty()
        {
            instance.loader_version.clone_from(version);
        }
        if let Some(key) = &self.downloaded
            && *key == instance.download_key()
        {
            instance.is_fully_downloaded = true;
            instance.cached_at = Some(Timestamp::now());
            instance.download_cache_key.clone_from(key);
        }
        if self.starter_mods_installed {
            instance.install_starter_fabric_mods = false;
        }
        if let Some(stale) = &self.stale_java
            && instance.java_path.as_deref() == Some(stale.path.as_path())
        {
            instance.java_path = None;
        }
    }
}

impl Launcher {
    /// A launcher for Mojang's, Fabric's, Modrinth's, and Adoptium's real
    /// services.
    pub fn new(http: Http, data: DataDir, runtimes: Runtimes) -> Self {
        Self {
            mojang: Mojang::new(http.clone(), &data),
            fabric: Fabric::new(http.clone(), &data),
            modrinth: modrinth::Client::new(http.clone()),
            http,
            runtimes,
            data,
            resources_url: RESOURCES_URL.to_owned(),
            stop_grace: DEFAULT_STOP_GRACE,
        }
    }

    /// Launches `instance` as `identity`: gets the version's metadata (from
    /// the cache when the services can't be reached), finds Java (installing
    /// it if none fits), installs the starter mods a new Fabric instance
    /// asked for, plans the launch, prepares its files, and starts the game.
    ///
    /// Reports each [`Step`] to `on_step`. Dropping the future before the
    /// game starts cancels the launch, leaving no partial files.
    pub async fn launch(
        &self,
        instance: &Instance,
        config: &Config,
        identity: &Identity,
        on_step: impl Fn(Step) + Send + Sync,
    ) -> Result<Launched> {
        let mut changes = InstanceChanges::default();

        on_step(Step::Version);
        let (version, picked_loader) = self.resolve(instance).await?;
        let Fetched {
            value: version,
            stale,
        } = version;
        // Launched as if the picked loader version were saved already, so
        // the download mark is for it.
        let instance = match &picked_loader {
            Some(loader_version) => Cow::Owned(Instance {
                loader_version: loader_version.clone(),
                ..instance.clone()
            }),
            None => Cow::Borrowed(instance),
        };
        let instance = instance.as_ref();
        changes.loader_version = picked_loader;

        on_step(Step::Java);
        let (java, stale_java) = self.java(&version, instance, config, &on_step).await?;
        changes.stale_java = stale_java;

        let starter_mods =
            if instance.loader == Loader::Fabric && instance.install_starter_fabric_mods {
                let report = self
                    .modrinth
                    .install_starter(instance, |p| on_step(Step::StarterMods(p)))
                    .await;
                changes.starter_mods_installed = report.all_ok();
                Some(report)
            } else {
                None
            };

        let mut plan = plan(&PlanInputs {
            version: &version,
            instance,
            config,
            data: &self.data,
            java: &java.java,
            identity,
            // Libraries are for the Java's architecture: on Apple silicon,
            // an Intel Java loads Intel natives.
            env: &Env::new(self.runtimes.os, java.arch),
        })?;
        if let Some(assets) = &mut plan.assets {
            assets.objects_url.clone_from(&self.resources_url);
        }
        tracing::debug!(?plan, "launching");

        let prepared = prepare(&self.http, &plan, &on_step).await?;
        changes.downloaded = prepared.verified.then(|| plan.download_key.clone());

        on_step(Step::Starting);
        let (game, logs) = Game::spawn(plan.command(), self.stop_grace)?;
        Ok(Launched {
            game,
            logs,
            changes,
            java,
            stale,
            starter_mods,
        })
    }

    /// The version to launch, with the instance's loader merged in, and the
    /// Fabric loader version picked if the instance had none.
    async fn resolve(&self, instance: &Instance) -> Result<(Fetched<VersionJson>, Option<String>)> {
        let game = &instance.version;
        match &instance.loader {
            Loader::Vanilla => Ok((self.mojang.resolve(game).await?, None)),
            Loader::Fabric => {
                let chosen = instance.loader_version.trim();
                let picked = if chosen.is_empty() {
                    let versions = self.fabric.loader_versions(game).await?;
                    let default = fabric::default_loader(&versions)
                        .ok_or_else(|| Error::NoFabricLoader(game.clone()))?;
                    Some(default.version.clone())
                } else {
                    None
                };
                let loader = picked.as_deref().unwrap_or(chosen);
                let version = self.fabric.resolve(&self.mojang, game, loader).await?;
                Ok((version, picked))
            }
            other => Err(Error::UnsupportedLoader(other.display_name().to_owned())),
        }
    }

    /// The Java to run `version` with: the instance's or the app's if set
    /// (an error if it can't run it), else an installed one that fits, else
    /// a fresh install. Also the instance's own Java, if it was set aside as
    /// stale.
    async fn java(
        &self,
        version: &VersionJson,
        instance: &Instance,
        config: &Config,
        on_step: &(impl Fn(Step) + Sync),
    ) -> Result<(Runtime, Option<StaleJava>)> {
        let needs = self.runtimes.needs(version);
        let choice = Choice::of(instance, config);
        let Pick { runtime, stale } = self.runtimes.find(&needs, choice).await?;
        if let Some(runtime) = runtime {
            return Ok((runtime, stale));
        }
        let install = self.runtimes.install(&self.http, &needs, |progress| {
            on_step(Step::InstallingJava(progress));
        });
        Ok((install.await?, stale))
    }
}
