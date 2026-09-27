//! Mods: what an instance has installed, and Modrinth to find more.
//!
//! Installed sits on the left; Discover on the right is a search field over
//! the results, with the selected result's details under them (or in a
//! column of their own when there's room). Below 76 columns one pane shows at
//! a time. Listing, searching and installing run in the background and report
//! back as [`Msg`]s. One install runs at a time, and esc cancels it in place.

mod copy;
mod input;
mod layout;
mod view;

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use mctui_core::instance::Instance;
use mctui_core::modrinth::{self, InstallReport, InstalledMod, SearchHit, SearchResults};
use mctui_core::net::Progress;
use ratatui::layout::Rect;

use crate::app::{Action, Banner};
use crate::data::Data;
use crate::overlay::{HelpGroup, PaletteItem};
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::task::{Task, Tasks};
use crate::ui::input::TextInput;
use crate::ui::list::List;
use crate::ui::{self, Canvas, Hint};
use copy::{banner, hit_page, mod_page, open_page};

/// How long typing must pause before the query is searched.
const DEBOUNCE: Duration = Duration::from_millis(350);
/// Results asked for at a time.
const PAGE: usize = 20;
/// The next page is fetched when the selection gets this close to the end.
const LOAD_AHEAD: usize = 5;

/// A pane, for clicks and the compact layout's one-at-a-time view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Installed,
    Discover,
}

/// Where keys go. Discover holds two stops: the search field and the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Installed,
    Search,
    Results,
}

impl Focus {
    fn pane(self) -> Pane {
        match self {
            Focus::Installed => Pane::Installed,
            Focus::Search | Focus::Results => Pane::Discover,
        }
    }

    /// Tab order: Installed → search → results, round again.
    fn cycle(self, forward: bool) -> Self {
        match (self, forward) {
            (Focus::Installed, true) | (Focus::Results, false) => Focus::Search,
            (Focus::Search, true) | (Focus::Installed, false) => Focus::Results,
            (Focus::Results, true) | (Focus::Search, false) => Focus::Installed,
        }
    }
}

/// The screen's clickable parts, besides list rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A pane's background, or the compact layout's switch to it.
    Pane(Pane),
    Search,
    /// The details' Install / Update button.
    Install,
    /// The details' Modrinth page button.
    Page,
}

/// Background work reporting back, and choices made in menus and dialogs.
#[derive(Debug)]
pub enum Msg {
    Listed(Vec<InstalledMod>),
    Found {
        seq: u64,
        result: modrinth::Result<SearchResults>,
    },
    Progress {
        seq: u64,
        progress: Progress,
    },
    Installed {
        seq: u64,
        result: modrinth::Result<InstallReport>,
    },
    Removed {
        title: String,
        result: modrinth::Result<()>,
    },
    /// Picked in a menu, a dialog or the palette.
    Chose(Command),
}

impl From<Msg> for crate::msg::Msg {
    fn from(msg: Msg) -> Self {
        Self::Mods(msg)
    }
}

/// Something to do, picked in a menu, a dialog or the palette (they reach the
/// screen through [`Action::Mods`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Install { project_id: String, title: String },
    Remove { file: String, title: String },
    Search,
}

#[derive(Debug)]
pub struct Mods {
    inst: Instance,
    client: modrinth::Client,
    tasks: Tasks,
    body: Rect,
    focus: Focus,
    installed: Installed,
    discover: Discover,
    install: Option<Install>,
    /// Warnings from the last install, shown in Discover until the next
    /// install or search.
    notes: Vec<String>,
    /// The last tick. Spinners are drawn as of then, so a frame never
    /// depends on when it happens to be drawn.
    now: Instant,
    /// Numbers searches and installs, so a superseded one's late answer is
    /// dropped.
    seq: u64,
}

#[derive(Debug)]
struct Installed {
    /// `None` until the mods folder has been read.
    mods: Option<Vec<InstalledMod>>,
    list: List,
    listing: Option<Task>,
    removing: Option<Task>,
}

#[derive(Debug)]
struct Discover {
    input: TextInput,
    /// When what's typed is due to be searched.
    due: Option<Instant>,
    /// The query the results are for.
    query: String,
    hits: Vec<SearchHit>,
    /// How many projects match, across pages.
    total: usize,
    /// Where the next page starts.
    next_offset: usize,
    list: List,
    search: Search,
}

#[derive(Debug)]
enum Search {
    Idle,
    Loading {
        seq: u64,
        /// Fetching the next page, not a new search.
        more: bool,
        offset: usize,
        started: Instant,
        _task: Task,
    },
    Failed {
        more: bool,
        message: String,
    },
}

#[derive(Debug)]
struct Install {
    seq: u64,
    project_id: String,
    title: String,
    /// It was installed already: this updates it.
    update: bool,
    /// `None` while dependencies are still being worked out.
    progress: Option<Progress>,
    started: Instant,
    _task: Task,
}

impl Mods {
    /// The screen for `inst`, reporting through `tasks` (addressed to it).
    pub fn new(inst: &Instance, tasks: Tasks, client: modrinth::Client) -> Self {
        let mut mods = Self {
            inst: inst.clone(),
            client,
            tasks,
            body: Rect::default(),
            focus: Focus::Results,
            installed: Installed {
                mods: None,
                list: List::new("mods-installed"),
                listing: None,
                removing: None,
            },
            discover: Discover {
                input: TextInput::default(),
                due: None,
                query: String::new(),
                hits: Vec::new(),
                total: 0,
                next_offset: 0,
                list: List::new("mods-results"),
                search: Search::Idle,
            },
            install: None,
            notes: Vec::new(),
            now: Instant::now(),
            seq: 0,
        };
        mods.list_installed();
        mods.search(false);
        mods
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    // --- Background work ------------------------------------------------------

    /// Reads the mods folder again.
    fn list_installed(&mut self) {
        let inst = self.inst.clone();
        let task = self
            .tasks
            .run(async move { modrinth::installed(&inst).await }, |mods| {
                Msg::Listed(mods).into()
            });
        self.installed.listing = Some(task);
    }

    /// Searches what's typed (an empty query browses the most downloaded), or
    /// with `more`, fetches the next page of the current results.
    fn search(&mut self, more: bool) {
        let d = &mut self.discover;
        d.due = None;
        let offset = if more {
            d.next_offset
        } else {
            d.query = d.input.as_str().trim().to_owned();
            0
        };
        if !more {
            self.notes.clear();
        }
        let seq = self.next_seq();
        let (client, query, version, loader) = (
            self.client.clone(),
            self.discover.query.clone(),
            self.inst.version.clone(),
            self.inst.loader.as_str().to_owned(),
        );
        let task = self.tasks.run(
            async move { client.search(&query, &version, &loader, offset, PAGE).await },
            move |result| Msg::Found { seq, result }.into(),
        );
        self.discover.search = Search::Loading {
            seq,
            more,
            offset,
            started: Instant::now(),
            _task: task,
        };
    }

    /// Fetches the next page when the selection nears the end of the results.
    fn maybe_load_more(&mut self) {
        let d = &self.discover;
        let near_end = d.list.selected() + LOAD_AHEAD >= d.hits.len();
        if near_end && d.next_offset < d.total && matches!(d.search, Search::Idle) {
            self.search(true);
        }
    }

    fn found(&mut self, seq: u64, result: modrinth::Result<SearchResults>) -> Handled {
        let d = &mut self.discover;
        let (more, offset) = match d.search {
            Search::Loading {
                seq: s,
                more,
                offset,
                ..
            } if s == seq => (more, offset),
            _ => return Handled::Ignored,
        };
        match result {
            Ok(page) => {
                if !more {
                    d.hits.clear();
                }
                // Pages can overlap when the ranking shifts between them.
                for hit in page.hits {
                    if !d.hits.iter().any(|h| h.project_id == hit.project_id) {
                        d.hits.push(hit);
                    }
                }
                d.total = page.total_hits;
                d.next_offset = offset + PAGE;
                d.search = Search::Idle;
            }
            Err(e) => {
                // A failed new search shows why in place of the old results.
                if !more {
                    d.hits.clear();
                    d.total = 0;
                }
                d.search = Search::Failed {
                    more,
                    message: copy::search_error(&e, more),
                };
            }
        }
        d.list
            .set_keys(d.hits.iter().map(|h| h.project_id.clone()).collect());
        if !more {
            d.list.select(0);
        }
        Handled::Consumed
    }

    /// Installs (or updates) a project, unless an install is running already.
    fn install(&mut self, project_id: &str, title: &str) -> Handled {
        if let Some(running) = &self.install {
            return banner(Banner::info(format!(
                "{} is still installing — one install at a time.",
                running.title
            )));
        }
        let seq = self.next_seq();
        let (client, inst, project) = (
            self.client.clone(),
            self.inst.clone(),
            project_id.to_owned(),
        );
        let task = self.tasks.spawn(move |mail| async move {
            let result = client
                .install(&inst, &project, |progress| {
                    mail.send(Msg::Progress { seq, progress }.into());
                })
                .await;
            mail.send(Msg::Installed { seq, result }.into());
        });
        self.notes.clear();
        self.install = Some(Install {
            seq,
            project_id: project_id.to_owned(),
            title: title.to_owned(),
            update: self.is_installed(project_id),
            progress: None,
            started: Instant::now(),
            _task: task,
        });
        Handled::Consumed
    }

    fn installed(&mut self, seq: u64, result: modrinth::Result<InstallReport>) -> Handled {
        let Some(install) = self.install.take_if(|i| i.seq == seq) else {
            return Handled::Ignored;
        };
        let (title, version) = (&install.title, &self.inst.version);
        let toast = match result {
            Ok(report) => {
                self.notes = copy::warnings(&report, title, version);
                Banner::success(copy::installed_toast(&report, title, install.update))
            }
            Err(e) => Banner::warn(copy::install_error(&e, title, version)),
        };
        // Reloading refreshes Installed here, and home's counts.
        Handled::Action(Action::Many(vec![Action::Banner(toast), Action::Reload]))
    }

    /// Stops the running install. What it finished (a dependency, say) stays.
    fn cancel_install(&mut self) -> Handled {
        match self.install.take() {
            Some(install) => Handled::Action(Action::Many(vec![
                Action::Banner(Banner::info(format!(
                    "Stopped installing {}.",
                    install.title
                ))),
                Action::Reload,
            ])),
            None => Handled::Ignored,
        }
    }

    fn remove(&mut self, file: &str, title: &str) -> Handled {
        if let Some(running) = &self.install {
            return banner(Banner::info(format!(
                "Wait for {} to finish installing, then remove {title}.",
                running.title
            )));
        }
        if self.installed.removing.is_some() {
            return banner(Banner::info("Still removing the last mod — try again."));
        }
        let (inst, file, title) = (self.inst.clone(), file.to_owned(), title.to_owned());
        let task = self.tasks.run(
            async move { modrinth::remove(&inst, &file).await },
            move |result| Msg::Removed { title, result }.into(),
        );
        self.installed.removing = Some(task);
        Handled::Consumed
    }

    fn on_msg(&mut self, msg: Msg) -> Handled {
        match msg {
            Msg::Listed(mods) => {
                self.installed.listing = None;
                self.installed
                    .list
                    .set_keys(mods.iter().map(|m| m.file.clone()).collect());
                self.installed.mods = Some(mods);
                Handled::Consumed
            }
            Msg::Found { seq, result } => self.found(seq, result),
            Msg::Progress { seq, progress } => match &mut self.install {
                Some(install) if install.seq == seq => {
                    install.progress = Some(progress);
                    Handled::Consumed
                }
                _ => Handled::Ignored,
            },
            Msg::Installed { seq, result } => self.installed(seq, result),
            Msg::Removed { title, result } => {
                self.installed.removing = None;
                let toast = match result {
                    Ok(()) => Banner::success(format!("Removed {title}.")),
                    Err(e) => Banner::warn(format!("Couldn't remove {title}: {e}")),
                };
                Handled::Action(Action::Many(vec![Action::Banner(toast), Action::Reload]))
            }
            Msg::Chose(Command::Install { project_id, title }) => {
                self.focus = Focus::Results;
                self.install(&project_id, &title)
            }
            Msg::Chose(Command::Remove { file, title }) => self.remove(&file, &title),
            Msg::Chose(Command::Search) => self.focus(Focus::Search),
        }
    }

    // --- Lookups -----------------------------------------------------------------

    fn selected_hit(&self) -> Option<&SearchHit> {
        self.discover.hits.get(self.discover.list.selected())
    }

    fn selected_mod(&self) -> Option<&InstalledMod> {
        let mods = self.installed.mods.as_deref()?;
        mods.get(self.installed.list.selected())
    }

    /// The installed jar of a search result's project, if it's installed.
    fn installed_jar(&self, hit: &SearchHit) -> Option<&InstalledMod> {
        let mods = self.installed.mods.as_deref()?;
        mods.iter()
            .find(|m| m.is_project(&hit.project_id, &hit.slug))
    }

    fn is_installed(&self, project_id: &str) -> bool {
        match self
            .discover
            .hits
            .iter()
            .find(|h| h.project_id == project_id)
        {
            Some(hit) => self.installed_jar(hit).is_some(),
            None => self
                .installed
                .mods
                .iter()
                .flatten()
                .any(|m| m.project_id.as_deref() == Some(project_id)),
        }
    }

    fn installing(&self, hit: &SearchHit) -> bool {
        self.install
            .as_ref()
            .is_some_and(|i| i.project_id == hit.project_id)
    }
}

impl Screen for Mods {
    fn crumbs(&self, _data: &Data) -> Vec<String> {
        vec![self.inst.name.clone(), "Mods".into()]
    }

    fn hints(&self, _data: &Data) -> Vec<Hint> {
        let key = Hint::key;
        if self.focus == Focus::Search {
            return vec![
                key(KeyCode::Enter, "search"),
                key(KeyCode::Down, "results"),
                key(KeyCode::Tab, "next"),
                key(KeyCode::Esc, "done"),
            ];
        }
        let mut hints = Vec::new();
        if self.focus == Focus::Installed {
            if let Some(m) = self.selected_mod() {
                hints.push(key(KeyCode::Char('d'), "remove"));
                if mod_page(m).is_some() {
                    hints.push(key(KeyCode::Char('o'), "page"));
                }
            }
            hints.push(key(KeyCode::Right, "discover"));
        } else {
            if let Some(hit) = self.selected_hit() {
                if !self.installing(hit) {
                    let verb = if self.installed_jar(hit).is_some() {
                        "update"
                    } else {
                        "install"
                    };
                    hints.push(key(KeyCode::Enter, verb));
                }
                hints.push(key(KeyCode::Char('o'), "page"));
            }
            if matches!(self.discover.search, Search::Failed { .. }) {
                hints.push(key(KeyCode::Char('r'), "retry"));
            }
            hints.push(key(KeyCode::Char('/'), "search"));
        }
        let esc = if self.install.is_some() {
            "cancel install"
        } else {
            "back"
        };
        hints.push(key(KeyCode::Esc, esc));
        hints.push(key(KeyCode::Char('?'), "help"));
        hints
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        ("Mods", copy::help())
    }

    fn palette(&self, _data: &Data) -> Vec<PaletteItem> {
        let verb = |label: String, key, action| PaletteItem {
            label,
            key: Some(key),
            jump: false,
            action,
        };
        let mut items = vec![verb(
            "Search Modrinth".into(),
            "/",
            Action::Mods(Command::Search),
        )];
        if let Some(hit) = self.selected_hit() {
            let title = &hit.title;
            let install = if self.installed_jar(hit).is_some() {
                "Update"
            } else {
                "Install"
            };
            let project_id = hit.project_id.clone();
            items.extend([
                verb(
                    format!("{install} {title}"),
                    "↵",
                    Action::Mods(Command::Install {
                        project_id,
                        title: title.clone(),
                    }),
                ),
                verb(
                    format!("Open {title} on Modrinth"),
                    "o",
                    open_page(hit_page(hit), title),
                ),
            ]);
        }
        if let Some(m) = self.selected_mod() {
            items.push(verb(
                format!("Remove {}", m.title),
                "d",
                self.confirm_remove(m),
            ));
        }
        items
    }

    fn captures_text(&self) -> bool {
        self.focus == Focus::Search
    }

    fn set_body(&mut self, body: Rect) {
        self.body = body;
        let lay = layout::layout(body, self.focus.pane());
        if let Some(pane) = lay.installed {
            let rows = layout::installed(pane).list;
            self.installed.list.set_height(rows.height);
        }
        if let Some(pane) = lay.discover {
            let activity = self.activity_rows(pane.width);
            let parts = layout::discover(pane, activity, lay.details.is_none());
            self.discover.list.set_height(parts.list.height);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        self.on_key(key, cx.now)
    }

    fn handle_mouse(&mut self, target: &ui::Target, mouse: Mouse, _cx: &mut Ctx<'_>) -> Handled {
        self.on_mouse(target, mouse)
    }

    fn handle_paste(&mut self, text: &str, cx: &mut Ctx<'_>) -> Handled {
        self.on_paste(text, cx.now)
    }

    // Other screens' messages never come here; the wildcard stays right as
    // they add theirs.
    #[allow(unreachable_patterns, clippy::match_wildcard_for_single_variants)]
    fn handle_msg(&mut self, msg: crate::msg::Msg, _cx: &mut Ctx<'_>) -> Handled {
        match msg {
            crate::msg::Msg::Mods(msg) => self.on_msg(msg),
            _ => Handled::Ignored,
        }
    }

    fn reload(&mut self, data: &Data) {
        if let Some(inst) = data.instance(&self.inst.id) {
            self.inst = inst.clone();
        }
        self.list_installed();
    }

    fn next_deadline(&self) -> Option<Instant> {
        let spinning = match (&self.install, &self.discover.search) {
            (Some(install), _) => Some(install.started),
            (None, Search::Loading { started, .. }) => Some(*started),
            _ => None,
        };
        let spin = spinning.map(|started| ui::widgets::next_spin(started, self.now));
        spin.into_iter().chain(self.discover.due).min()
    }

    fn tick(&mut self, now: Instant) -> bool {
        self.now = now;
        let due = self.discover.due.is_some_and(|due| due <= now);
        if due {
            self.search(false);
        }
        due || self.install.is_some() || matches!(self.discover.search, Search::Loading { .. })
    }

    fn render(&self, cv: &mut Canvas<'_>, _data: &Data) {
        self.draw(cv);
    }
}

#[cfg(test)]
mod tests;
