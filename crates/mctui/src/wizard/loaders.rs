//! The loader step's model: the loaders on offer, and what Fabric has for
//! the chosen game version.

use std::time::Instant;

use mctui_core::fabric::{self, LoaderVersion};
use mctui_core::instance;

use super::WizardMsg;
use crate::msg::Msg;
use crate::services::Services;
use crate::task::{Task, Tasks};

/// The loader list's id, for its rows' click targets.
pub(super) const LIST: &str = "wizard.loaders";

/// A row of the loader step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoaderChoice {
    Fabric,
    Vanilla,
    Forge,
    Quilt,
    NeoForge,
}

impl LoaderChoice {
    pub(super) const ALL: [Self; 5] = [
        Self::Fabric,
        Self::Vanilla,
        Self::Forge,
        Self::Quilt,
        Self::NeoForge,
    ];

    /// The choice on row `i`.
    pub(super) fn at(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }

    /// Its list key.
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Vanilla => "vanilla",
            Self::Forge => "forge",
            Self::Quilt => "quilt",
            Self::NeoForge => "neoforge",
        }
    }

    pub(super) fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Vanilla => "Vanilla",
            Self::Forge => "Forge",
            Self::Quilt => "Quilt",
            Self::NeoForge => "NeoForge",
        }
    }

    /// Whether mctui can make instances with it yet.
    pub(super) fn supported(self) -> bool {
        matches!(self, Self::Fabric | Self::Vanilla)
    }

    pub(super) fn loader(self) -> instance::Loader {
        match self {
            Self::Fabric => instance::Loader::Fabric,
            Self::Vanilla => instance::Loader::Vanilla,
            Self::Forge => instance::Loader::Forge,
            Self::Quilt => instance::Loader::Quilt,
            Self::NeoForge => instance::Loader::NeoForge,
        }
    }
}

/// What Fabric offers for one game version.
#[derive(Debug)]
pub(super) struct FabricCheck {
    pub game: String,
    pub status: FabricStatus,
}

#[derive(Debug)]
pub(super) enum FabricStatus {
    Checking {
        started: Instant,
        _task: Task,
    },
    /// Its newest stable loader.
    Ready(String),
    /// Fabric doesn't support the game version.
    Unsupported,
    /// Fabric couldn't be reached.
    Failed,
}

impl FabricCheck {
    /// Asks Fabric about `game`; the answer arrives as
    /// [`WizardMsg::Loaders`].
    pub(super) fn start(game: &str, services: &Services, tasks: &Tasks, now: Instant) -> Self {
        let fabric = services.fabric.clone();
        let (asked, reply) = (game.to_owned(), game.to_owned());
        let task = tasks.run(
            async move { fabric.loader_versions(&asked).await },
            move |result| {
                Msg::Wizard(WizardMsg::Loaders {
                    game: reply,
                    result,
                })
            },
        );
        Self {
            game: game.to_owned(),
            status: FabricStatus::Checking {
                started: now,
                _task: task,
            },
        }
    }

    /// Fabric answered: its newest stable loader, or that it has none for
    /// this version.
    pub(super) fn answered(&mut self, result: fabric::Result<Vec<LoaderVersion>>) {
        self.status = match result {
            Ok(versions) => fabric::default_loader(&versions)
                .map_or(FabricStatus::Unsupported, |v| {
                    FabricStatus::Ready(v.version.clone())
                }),
            Err(error) => {
                tracing::warn!(%error, game = self.game, "couldn't get Fabric's loader versions");
                FabricStatus::Failed
            }
        };
    }

    /// When the spinner for this check started, while it runs.
    pub(super) fn started(&self) -> Option<Instant> {
        match self.status {
            FabricStatus::Checking { started, .. } => Some(started),
            _ => None,
        }
    }
}
