//! Keys, clicks and the right-click menus.
//!
//! tab cycles Installed → search field → results; ←→ move between the panes;
//! ↑ from the first result reaches the field and ↓ leaves it. `/` jumps to
//! the field from anywhere. esc leaves the field, then cancels an install,
//! then (unhandled) closes the screen.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mctui_core::modrinth::{InstalledMod, SearchHit};
use ratatui::layout::Position;

use super::copy::{banner, hit_page, mod_page, open_page};
use super::{Command, DEBOUNCE, Focus, Mods, Pane, Search, Target};
use crate::app::{Action, Banner, Question};
use crate::overlay::MenuItem;
use crate::screen::{Handled, Mouse};
use crate::ui;

impl Mods {
    pub(super) fn on_key(&mut self, key: KeyEvent, now: Instant) -> Handled {
        if self.focus == Focus::Search {
            return self.search_key(key, now);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Handled::Ignored;
        }
        match key.code {
            KeyCode::Esc => self.cancel_install(),
            KeyCode::Tab => self.focus(self.focus.cycle(true)),
            KeyCode::BackTab => self.focus(self.focus.cycle(false)),
            KeyCode::Char('/') => self.focus(Focus::Search),
            KeyCode::Char('r') => self.retry(),
            KeyCode::Right | KeyCode::Char('l') if self.focus == Focus::Installed => {
                self.focus(Focus::Results)
            }
            KeyCode::Left | KeyCode::Char('h') if self.focus == Focus::Results => {
                self.focus(Focus::Installed)
            }
            _ if self.focus == Focus::Installed => self.installed_key(key),
            _ => self.results_key(key),
        }
    }

    fn installed_key(&mut self, key: KeyEvent) -> Handled {
        if let Some(moved) = self.installed.list.handle_key(key) {
            return Handled::changed(moved);
        }
        let Some(m) = self.selected_mod() else {
            return Handled::Ignored;
        };
        match key.code {
            KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                Handled::Action(self.confirm_remove(m))
            }
            KeyCode::Char('o') => match mod_page(m) {
                Some(url) => Handled::Action(open_page(url, &m.title)),
                None => banner(Banner::info(format!(
                    "mctui didn't install {}, so it doesn't know its Modrinth page.",
                    m.title
                ))),
            },
            _ => Handled::Ignored,
        }
    }

    fn results_key(&mut self, key: KeyEvent) -> Handled {
        let at_top = self.discover.list.selected() == 0;
        if matches!(key.code, KeyCode::Up | KeyCode::Char('k')) && at_top {
            return self.focus(Focus::Search);
        }
        if let Some(moved) = self.discover.list.handle_key(key) {
            self.maybe_load_more();
            return Handled::changed(moved);
        }
        let Some(hit) = self.selected_hit() else {
            return Handled::Ignored;
        };
        match key.code {
            KeyCode::Enter => {
                let (id, title) = (hit.project_id.clone(), hit.title.clone());
                self.install(&id, &title)
            }
            KeyCode::Char('o') => Handled::Action(open_page(hit_page(hit), &hit.title)),
            _ => Handled::Ignored,
        }
    }

    /// Keys while typing a query: they edit it, but for ↵ (search now), ↓
    /// (to the results), tab and esc (leave the field).
    fn search_key(&mut self, key: KeyEvent, now: Instant) -> Handled {
        match key.code {
            KeyCode::Enter => {
                let d = &self.discover;
                let stale = d.due.is_some()
                    || d.input.as_str().trim() != d.query
                    || matches!(d.search, Search::Failed { .. });
                if stale {
                    self.search(false);
                }
                self.focus(Focus::Results)
            }
            KeyCode::Esc | KeyCode::Tab => self.focus(Focus::Results),
            KeyCode::Down if !self.discover.hits.is_empty() => self.focus(Focus::Results),
            KeyCode::BackTab => self.focus(Focus::Installed),
            KeyCode::Up | KeyCode::Down => Handled::Consumed,
            _ => match self.discover.input.handle_key(key) {
                Some(true) => {
                    self.edited(now);
                    Handled::Consumed
                }
                Some(false) => Handled::Consumed,
                None => Handled::Ignored,
            },
        }
    }

    pub(super) fn on_paste(&mut self, text: &str, now: Instant) -> Handled {
        self.focus = Focus::Search;
        if self.discover.input.insert(text) {
            self.edited(now);
        }
        Handled::Consumed
    }

    /// The query changed: search once typing pauses.
    fn edited(&mut self, now: Instant) {
        self.discover.due = Some(now + DEBOUNCE);
        self.notes.clear();
    }

    fn retry(&mut self) -> Handled {
        match self.discover.search {
            Search::Failed { more, .. } => {
                self.search(more);
                Handled::Consumed
            }
            _ => Handled::Ignored,
        }
    }

    pub(super) fn focus(&mut self, focus: Focus) -> Handled {
        self.focus = focus;
        Handled::Consumed
    }

    pub(super) fn on_mouse(&mut self, target: &ui::Target, mouse: Mouse) -> Handled {
        if let Some(key) = self.installed.list.key_of(target) {
            let key = key.to_owned();
            return self.installed_row(&key, mouse);
        }
        if let Some(key) = self.discover.list.key_of(target) {
            let key = key.to_owned();
            return self.result_row(&key, mouse);
        }
        let ui::Target::Mods(target) = target else {
            return Handled::Ignored;
        };
        match (target, mouse) {
            (Target::Pane(pane), Mouse::Scroll { down }) => self.scroll(*pane, down),
            (Target::Pane(Pane::Installed), Mouse::Click { .. }) => self.focus(Focus::Installed),
            (Target::Pane(Pane::Discover), Mouse::Click { .. }) => self.focus(Focus::Results),
            (Target::Search, Mouse::Click { .. }) => self.focus(Focus::Search),
            (Target::Install, Mouse::Click { .. }) => match self.selected_hit() {
                Some(hit) => {
                    let (id, title) = (hit.project_id.clone(), hit.title.clone());
                    self.focus = Focus::Results;
                    self.install(&id, &title)
                }
                None => Handled::Ignored,
            },
            (Target::Page, Mouse::Click { .. }) => match self.selected_hit() {
                Some(hit) => Handled::Action(open_page(hit_page(hit), &hit.title)),
                None => Handled::Ignored,
            },
            _ => Handled::Ignored,
        }
    }

    /// The wheel over a pane moves its selection, and focuses it so the
    /// selection shows.
    fn scroll(&mut self, pane: Pane, down: bool) -> Handled {
        let step = if down { 1 } else { -1 };
        match pane {
            Pane::Installed => {
                self.focus = Focus::Installed;
                self.installed.list.step(step);
            }
            Pane::Discover => {
                self.focus = Focus::Results;
                self.discover.list.step(step);
                self.maybe_load_more();
            }
        }
        Handled::Consumed
    }

    fn installed_row(&mut self, key: &str, mouse: Mouse) -> Handled {
        match mouse {
            Mouse::Scroll { down } => self.scroll(Pane::Installed, down),
            Mouse::Click { .. } => {
                self.installed.list.select_key(key);
                self.focus(Focus::Installed)
            }
            Mouse::RightClick(at) => {
                self.installed.list.select_key(key);
                self.focus = Focus::Installed;
                match self.selected_mod() {
                    Some(m) => Handled::Action(self.mod_menu(m, at)),
                    None => Handled::Consumed,
                }
            }
        }
    }

    fn result_row(&mut self, key: &str, mouse: Mouse) -> Handled {
        match mouse {
            Mouse::Scroll { down } => self.scroll(Pane::Discover, down),
            Mouse::Click { double } => {
                self.discover.list.select_key(key);
                self.focus = Focus::Results;
                self.maybe_load_more();
                match self.selected_hit() {
                    Some(hit) if double => {
                        let (id, title) = (hit.project_id.clone(), hit.title.clone());
                        self.install(&id, &title)
                    }
                    _ => Handled::Consumed,
                }
            }
            Mouse::RightClick(at) => {
                self.discover.list.select_key(key);
                self.focus = Focus::Results;
                match self.selected_hit() {
                    Some(hit) => Handled::Action(self.hit_menu(hit, at)),
                    None => Handled::Consumed,
                }
            }
        }
    }

    // --- Menus and dialogs -----------------------------------------------------------

    pub(super) fn confirm_remove(&self, m: &InstalledMod) -> Action {
        Action::Confirm(Box::new(Question {
            title: "Remove mod?".into(),
            message: format!("Remove {} from “{}”?", m.title, self.inst.name),
            yes: "Remove",
            danger: Some("Its file is deleted from the mods folder.".into()),
            on_yes: Action::Mods(Command::Remove {
                file: m.file.clone(),
                title: m.title.clone(),
            }),
        }))
    }

    /// The right-click menu for an installed mod.
    fn mod_menu(&self, m: &InstalledMod, at: Position) -> Action {
        let open = mod_page(m).map(|url| {
            let open = open_page(url, &m.title);
            menu_item("Open Modrinth page", KeyCode::Char('o'), open)
        });
        let remove = MenuItem {
            danger: true,
            ..menu_item("Remove", KeyCode::Char('d'), self.confirm_remove(m))
        };
        Action::Menu {
            title: m.title.clone(),
            items: open.into_iter().chain([remove]).collect(),
            anchor: Some(at),
        }
    }

    /// The right-click menu for a search result.
    fn hit_menu(&self, hit: &SearchHit, at: Position) -> Action {
        let installed = self.installed_jar(hit);
        let install = Action::Mods(Command::Install {
            project_id: hit.project_id.clone(),
            title: hit.title.clone(),
        });
        let open = open_page(hit_page(hit), &hit.title);
        let mut items = vec![
            menu_item(
                if installed.is_some() {
                    "Update"
                } else {
                    "Install"
                },
                KeyCode::Enter,
                install,
            ),
            menu_item("Open Modrinth page", KeyCode::Char('o'), open),
        ];
        if let Some(m) = installed {
            items.push(MenuItem {
                danger: true,
                ..menu_item("Remove", KeyCode::Char('d'), self.confirm_remove(m))
            });
        }
        Action::Menu {
            title: hit.title.clone(),
            items,
            anchor: Some(at),
        }
    }
}

fn menu_item(label: &'static str, key: KeyCode, action: Action) -> MenuItem {
    MenuItem {
        label,
        key,
        danger: false,
        action,
    }
}
