//! The version step's model: Mojang's list, the snapshot toggle, the filter,
//! and the rows they leave on screen.

use std::time::Instant;

use mctui_core::mojang::{self, Fetched, Manifest, VersionEntry, VersionKind};

use super::WizardMsg;
use crate::msg::Msg;
use crate::services::Services;
use crate::task::{Task, Tasks};
use crate::ui::input::TextInput;
use crate::ui::list::List;

/// The version list's id, for its rows' click targets.
pub(super) const LIST: &str = "wizard.versions";

/// Fetching the list in the background.
#[derive(Debug)]
pub(super) enum Fetch {
    Idle,
    Running { started: Instant, _task: Task },
    Failed,
}

impl Fetch {
    pub(super) fn started(&self) -> Option<Instant> {
        match self {
            Fetch::Running { started, .. } => Some(*started),
            Fetch::Idle | Fetch::Failed => None,
        }
    }
}

#[derive(Debug, Default)]
pub(super) enum Filter {
    #[default]
    Off,
    /// Typing a query; keys go to the field.
    Editing(TextInput),
    /// A query narrows the list; keys work as usual.
    Applied(TextInput),
}

impl Filter {
    pub(super) fn query(&self) -> &str {
        match self {
            Filter::Off => "",
            Filter::Editing(q) | Filter::Applied(q) => q.as_str(),
        }
    }
}

/// A version on screen: where it is in the manifest, and the characters the
/// filter matched.
#[derive(Debug)]
pub(super) struct Row {
    pub index: usize,
    pub positions: Vec<u32>,
}

#[derive(Debug)]
pub(super) struct Versions {
    /// The last list that arrived.
    pub manifest: Option<Manifest>,
    /// That list is the saved copy: Mojang couldn't be reached.
    pub stale: bool,
    pub fetch: Fetch,
    /// Snapshots and old betas and alphas are listed too, not just releases.
    pub all: bool,
    pub filter: Filter,
    pub rows: Vec<Row>,
    pub list: List,
}

impl Versions {
    pub(super) fn new(all: bool) -> Self {
        Self {
            manifest: None,
            stale: false,
            fetch: Fetch::Idle,
            all,
            filter: Filter::Off,
            rows: Vec::new(),
            list: List::new(LIST),
        }
    }

    /// Asks Mojang for the list; it arrives as [`WizardMsg::Versions`].
    pub(super) fn fetch(&mut self, services: &Services, tasks: &Tasks, now: Instant) {
        let mojang = services.mojang.clone();
        let task = tasks.run(async move { mojang.manifest().await }, |result| {
            Msg::Wizard(WizardMsg::Versions(result))
        });
        self.fetch = Fetch::Running {
            started: now,
            _task: task,
        };
    }

    /// The list arrived (or didn't). The first one opens on the latest
    /// release; later ones keep the selection.
    pub(super) fn arrived(&mut self, result: mojang::Result<Fetched<Manifest>>) {
        self.fetch = Fetch::Idle;
        match result {
            Ok(Fetched { value, stale }) => {
                let first = self.manifest.is_none();
                let latest = value.latest.release.clone();
                self.manifest = Some(value);
                self.stale = stale;
                self.refresh();
                if first {
                    self.list.select_key(&latest);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "couldn't load the Minecraft versions");
                if self.manifest.is_none() {
                    self.fetch = Fetch::Failed;
                }
            }
        }
    }

    /// Rebuilds the rows after the list, the snapshot toggle or the filter
    /// changed, keeping the same version selected if it's still shown.
    pub(super) fn refresh(&mut self) {
        let Some(manifest) = &self.manifest else {
            return;
        };
        let query = self.filter.query().trim();
        let mut ranked: Vec<(bool, Row)> = manifest
            .versions
            .iter()
            .enumerate()
            .filter(|(_, v)| self.all || v.kind == VersionKind::Release)
            .filter_map(|(index, v)| {
                let (prefix, positions) = matches(&v.id, query)?;
                Some((!prefix, Row { index, positions }))
            })
            .collect();
        // Versions starting with the query first, then the rest; newest
        // first within each.
        ranked.sort_unstable_by_key(|(later, row)| (*later, row.index));
        self.rows = ranked.into_iter().map(|(_, row)| row).collect();
        let keys = self
            .rows
            .iter()
            .map(|r| manifest.versions[r.index].id.clone())
            .collect();
        self.list.set_keys(keys);
    }

    /// The filter's text changed: rebuild, and highlight the best match.
    pub(super) fn requery(&mut self) {
        self.refresh();
        self.list.select(0);
    }

    pub(super) fn selected(&self) -> Option<&VersionEntry> {
        let row = self.rows.get(self.list.selected())?;
        self.manifest.as_ref()?.versions.get(row.index)
    }

    /// How many versions the toggle lets through, before the filter.
    pub(super) fn total(&self) -> usize {
        self.manifest.as_ref().map_or(0, |m| {
            m.versions
                .iter()
                .filter(|v| self.all || v.kind == VersionKind::Release)
                .count()
        })
    }

    /// Stops typing into the filter, keeping what was typed.
    pub(super) fn commit_filter(&mut self) {
        if let Filter::Editing(input) = &mut self.filter {
            let query = std::mem::take(input);
            self.filter = if query.as_str().trim().is_empty() {
                Filter::Off
            } else {
                Filter::Applied(query)
            };
        }
    }

    pub(super) fn clear_filter(&mut self) {
        self.filter = Filter::Off;
        self.refresh();
    }

    /// Whether `r` would fetch the list again: it failed, or only the saved
    /// copy could be shown.
    pub(super) fn can_retry(&self) -> bool {
        match self.fetch {
            Fetch::Running { .. } => false,
            Fetch::Failed => true,
            Fetch::Idle => self.stale,
        }
    }
}

/// Whether version `id` contains the filter's `query` (ignoring case): if so,
/// whether it starts with it, and which characters matched. Version ids are
/// families (`1.21`, `1.21.8`), so "1.8" should find 1.8.9 before 1.21.8,
/// which fuzzy matching wouldn't. An empty query matches everything.
fn matches(id: &str, query: &str) -> Option<(bool, Vec<u32>)> {
    if query.is_empty() {
        return Some((true, Vec::new()));
    }
    let id: Vec<char> = id.chars().collect();
    let query: Vec<char> = query.chars().collect();
    let at = id
        .windows(query.len())
        .position(|w| w.iter().zip(&query).all(|(a, b)| a.eq_ignore_ascii_case(b)))?;
    let positions = (at..at + query.len()).map(|i| i as u32).collect();
    Some((at == 0, positions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_prefers_the_family() {
        assert_eq!(matches("1.8.9", "1.8"), Some((true, vec![0, 1, 2])));
        assert_eq!(matches("1.21.8", "1.8"), Some((false, vec![3, 4, 5])));
        assert_eq!(
            matches("1.14 Pre-Release 5", "pre"),
            Some((false, vec![5, 6, 7]))
        );
        assert_eq!(matches("1.18.2", "1.8"), None);
        assert_eq!(matches("1.8", "1.8.9"), None, "longer than the id");
        assert_eq!(matches("rd-132211", ""), Some((true, Vec::new())));
    }
}
