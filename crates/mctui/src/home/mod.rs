//! Home: the instance list and the selected instance's details.
//!
//! Focus lives in one pane at a time. ↑↓ move within it and stop at the ends;
//! ←→ step from the list into the detail tabs and back. Only the focused pane
//! shows a selection.

mod layout;
mod view;

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mctui_core::facets::{self, ModJar, ResourcePack, World};
use mctui_core::instance::Instance;
use ratatui::layout::Rect;

use crate::app::{Action, Banner, Route};
use crate::data::Data;
use crate::fuzzy;
use crate::overlay::{HelpGroup, MenuItem, PaletteItem};
use crate::play;
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::ui::filter::{Filter, FilterKey};
use crate::ui::list::List;
use crate::ui::{self, Canvas, Hint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    List,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Mods,
    ResourcePacks,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Overview, Tab::Mods, Tab::ResourcePacks];

    fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Mods => "Mods",
            Tab::ResourcePacks => "Resource packs",
        }
    }

    fn step(self, right: bool) -> Option<Tab> {
        let i = Self::ALL.iter().position(|t| *t == self)?;
        let j = if right {
            i.checked_add(1)?
        } else {
            i.checked_sub(1)?
        };
        Self::ALL.get(j).copied()
    }
}

/// The instance list's id, for its rows' click targets.
pub const LIST: &str = "home";

/// "+ New instance"'s key. The other rows are keyed by instance id, so a
/// click or a selection survives the list being rebuilt (a reload, a filter)
/// underneath it; ids are folder names, which can't hold a `/`.
const NEW: &str = "/new";

/// Home's clickable parts, besides the list's rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    List,
    Detail,
    Tab(Tab),
    Play,
    PlayOffline,
    /// The Mods / Resource packs preview: opens the full manager.
    Preview,
    /// The empty screen's "create your first instance".
    Create,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Row {
    /// The instance's id, its index into `Data::instances` as of the last
    /// refresh, and the filter's matched characters.
    Instance {
        id: String,
        index: usize,
        positions: Vec<u32>,
    },
    New,
}

impl Row {
    fn key(&self) -> &str {
        match self {
            Row::Instance { id, .. } => id,
            Row::New => NEW,
        }
    }
}

/// Cheap disk facts about one instance, read when it's selected.
#[derive(Debug, Default)]
struct Facets {
    worlds: Vec<World>,
    mods: Vec<ModJar>,
    packs: Vec<ResourcePack>,
}

#[derive(Debug)]
pub struct Home {
    rows: Vec<Row>,
    /// The rows' selection and scroll, by their keys.
    list: List,
    focus: Pane,
    tab: Tab,
    filter: Filter,
    facets: HashMap<String, Facets>,
    body: Rect,
    /// The empty screen's wink, picked once per run.
    splash: &'static str,
}

impl Home {
    pub fn new(data: &Data, splash: &'static str) -> Self {
        let mut home = Self {
            rows: Vec::new(),
            list: List::new(LIST),
            focus: Pane::List,
            tab: Tab::Overview,
            filter: Filter::Off,
            facets: HashMap::new(),
            body: Rect::default(),
            splash,
        };
        home.refresh(data);
        home
    }

    #[cfg(test)]
    pub fn set_splash(&mut self, splash: &'static str) {
        self.splash = splash;
    }

    /// Instances changed on disk: drop cached facts and rebuild the list.
    pub fn reload(&mut self, data: &Data) {
        self.facets.clear();
        self.refresh(data);
    }

    /// Rebuilds the visible rows (after a data or filter change), keeping the
    /// same instance selected wherever it moved to.
    fn refresh(&mut self, data: &Data) {
        let on_new = self.list.selected_key() == Some(NEW) && !data.instances.is_empty();

        let query = self.filter.query();
        self.rows = fuzzy::rank(query, data.instances.iter().map(|i| i.name.as_str()))
            .into_iter()
            .map(|m| Row::Instance {
                id: data.instances[m.index].id.clone(),
                index: m.index,
                positions: m.positions,
            })
            .collect();
        // "+ New instance" is always last, and never a search result.
        if query.trim().is_empty() {
            self.rows.push(Row::New);
        }

        self.list.set_keys(self.rows.iter().map(Row::key));
        // A filter hid "+ New instance": stay at the end of the list.
        if on_new && self.list.selected_key() != Some(NEW) {
            self.list.select(usize::MAX);
        }
        self.load_facets(data);
    }

    /// The body area changed (resize, a banner appeared).
    pub fn set_body(&mut self, body: Rect) {
        self.body = body;
        let lay = layout::layout(body);
        if lay.compact() {
            self.focus = Pane::List;
        }
        self.list.set_height(layout::list_rows(&lay).height);
    }

    /// Whether keys are going into a text field (so `?`, `:` and `q` type).
    pub fn captures_text(&self) -> bool {
        self.filter.is_editing()
    }

    pub fn selected_instance<'d>(&self, data: &'d Data) -> Option<&'d Instance> {
        match self.rows.get(self.list.selected())? {
            Row::Instance { index, .. } => data.instances.get(*index),
            Row::New => None,
        }
    }

    /// Follows up a movement: picking another instance returns focus to the
    /// list, so the detail pane never shows stale focus, and reads its facts.
    /// A movement only did something if the selection `moved`.
    fn after_move(&mut self, moved: bool, data: &Data) -> Handled {
        if moved {
            self.focus_list();
            self.load_facets(data);
        }
        Handled::changed(moved)
    }

    /// Reads the selected instance's facts off disk, the first time.
    fn load_facets(&mut self, data: &Data) {
        if let Some(inst) = self.selected_instance(data)
            && !self.facets.contains_key(&inst.id)
        {
            let facets = Facets {
                worlds: facets::worlds(inst),
                mods: facets::mod_jars(inst),
                packs: facets::resource_packs(inst),
            };
            self.facets.insert(inst.id.clone(), facets);
        }
    }

    fn scroll(&mut self, down: bool, data: &Data) -> Handled {
        let moved = self.list.step(if down { 1 } else { -1 });
        self.after_move(moved, data)
    }

    fn on_paste(&mut self, text: &str, data: &Data) -> bool {
        let Some(changed) = self.filter.paste(text) else {
            return false;
        };
        if changed {
            self.refresh(data);
        }
        true
    }

    fn on_key(&mut self, key: KeyEvent, data: &Data) -> Handled {
        if let Some(event) = self.filter.handle_key(key) {
            return self.filtered(event, data);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Handled::Ignored;
        }
        if self.focus == Pane::Detail {
            match key.code {
                KeyCode::Esc => {
                    self.focus_list();
                    return Handled::Consumed;
                }
                // The previews aren't row-navigable; don't move the hidden list.
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Char('j' | 'k' | 'g' | 'G') => return Handled::Consumed,
                KeyCode::Enter => return self.activate_tab(data),
                KeyCode::Char('/') => self.focus_list(),
                _ => {}
            }
        }
        if let Some(handled) = self.navigate(key, data) {
            return handled;
        }

        let action = match key.code {
            KeyCode::Enter => match self.selected_instance(data) {
                Some(inst) => Action::launch(inst, false),
                None => Action::Open(Route::NewInstance),
            },
            KeyCode::Char('n') => Action::Open(Route::NewInstance),
            KeyCode::Char('s') => Action::Open(Route::Settings),
            KeyCode::Char('a') => Action::Open(Route::Accounts),
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('/') => {
                if !data.instances.is_empty() {
                    self.focus_list();
                    self.filter.edit();
                }
                return Handled::Consumed;
            }
            KeyCode::Esc if matches!(self.filter, Filter::Off) => {
                Action::Banner(Banner::info("Press q to quit."))
            }
            KeyCode::Esc => {
                self.filter = Filter::Off;
                self.refresh(data);
                return Handled::Consumed;
            }
            code => match self
                .selected_instance(data)
                .and_then(|inst| instance_action(inst, code))
            {
                Some(action) => action,
                None => return Handled::Ignored,
            },
        };
        Handled::Action(action)
    }

    /// What a key did to the filter being typed.
    fn filtered(&mut self, event: FilterKey, data: &Data) -> Handled {
        match event {
            FilterKey::Step(delta) => {
                let moved = self.list.step(delta);
                return self.after_move(moved, data);
            }
            FilterKey::Changed | FilterKey::Committed | FilterKey::Cleared => self.refresh(data),
            FilterKey::Consumed => {}
        }
        Handled::Consumed
    }

    /// Moving between and within panes.
    fn navigate(&mut self, key: KeyEvent, data: &Data) -> Option<Handled> {
        let split = !layout::layout(self.body).compact();
        let handled = match key.code {
            KeyCode::Right | KeyCode::Char('l' | ']') if split => {
                match self.focus {
                    Pane::List => self.focus_detail(data),
                    Pane::Detail => self.tab = self.tab.step(true).unwrap_or(self.tab),
                }
                Handled::Consumed
            }
            KeyCode::Left | KeyCode::Char('h' | '[') if split => {
                if self.focus == Pane::Detail {
                    match self.tab.step(false) {
                        Some(tab) => self.tab = tab,
                        None => self.focus_list(),
                    }
                }
                Handled::Consumed
            }
            KeyCode::Tab if split => {
                match self.focus {
                    Pane::List => self.focus_detail(data),
                    Pane::Detail => self.focus_list(),
                }
                Handled::Consumed
            }
            _ => {
                let moved = self.list.handle_key(key)?;
                self.after_move(moved, data)
            }
        };
        Some(handled)
    }

    /// Stops typing into the filter, keeping what was typed.
    pub fn commit_filter(&mut self, data: &Data) {
        if self.filter.commit() {
            self.refresh(data);
        }
    }

    /// A click stops typing into the filter, keeping what was typed; the
    /// wheel doesn't.
    fn pointer(&mut self, mouse: Mouse, data: &Data) {
        if !matches!(mouse, Mouse::Scroll { .. }) {
            self.commit_filter(data);
        }
    }

    /// The pointer on the row keyed `key`.
    fn on_row(&mut self, key: &str, mouse: Mouse, data: &Data) -> Handled {
        self.pointer(mouse, data);
        if let Mouse::Scroll { down } = mouse {
            return self.scroll(down, data);
        }
        let moved = self.list.select_key(key);
        if self.list.selected_key() != Some(key) {
            // Gone since the frame it was clicked on.
            return Handled::Ignored;
        }
        self.after_move(moved, data);
        self.focus_list();
        let inst = self.selected_instance(data);
        match (mouse, inst) {
            (Mouse::Click { double: true }, Some(inst)) => {
                Handled::Action(Action::launch(inst, false))
            }
            (Mouse::Click { double: true }, None) => {
                Handled::Action(Action::Open(Route::NewInstance))
            }
            (Mouse::RightClick(at), Some(inst)) => Handled::Action(Action::ActionsMenu {
                id: inst.id.clone(),
                anchor: Some(at),
            }),
            _ => Handled::Consumed,
        }
    }

    fn on_mouse(&mut self, target: &Target, mouse: Mouse, data: &Data) -> Handled {
        self.pointer(mouse, data);
        match (target, mouse) {
            (Target::List, Mouse::Scroll { down }) => self.scroll(down, data),
            (Target::List, Mouse::Click { .. }) => {
                self.focus_list();
                Handled::Consumed
            }
            (Target::Detail, Mouse::Click { .. }) => {
                self.focus_detail(data);
                Handled::Consumed
            }
            (Target::Tab(tab), Mouse::Click { .. }) => {
                self.focus_detail(data);
                self.tab = *tab;
                Handled::Consumed
            }
            (Target::Play, Mouse::Click { .. }) => self.launch(data, false),
            (Target::PlayOffline, Mouse::Click { .. }) => self.launch(data, true),
            (Target::Preview, Mouse::Click { .. }) => self.activate_tab(data),
            (Target::Create, Mouse::Click { .. }) => {
                Handled::Action(Action::Open(Route::NewInstance))
            }
            _ => Handled::Ignored,
        }
    }

    fn focus_list(&mut self) {
        self.focus = Pane::List;
        self.tab = Tab::Overview;
    }

    fn focus_detail(&mut self, data: &Data) {
        if self.selected_instance(data).is_some() && !layout::layout(self.body).compact() {
            self.focus = Pane::Detail;
        }
    }

    fn launch(&self, data: &Data, offline: bool) -> Handled {
        match self.selected_instance(data) {
            Some(inst) => Handled::Action(Action::launch(inst, offline)),
            None => Handled::Ignored,
        }
    }

    /// Enter in the detail pane: Overview plays; the previews open their manager.
    fn activate_tab(&self, data: &Data) -> Handled {
        let Some(inst) = self.selected_instance(data) else {
            return Handled::Ignored;
        };
        Handled::Action(match self.tab {
            Tab::Overview => Action::launch(inst, false),
            Tab::Mods => mods(inst),
            Tab::ResourcePacks => Action::Open(Route::ResourcePacks(inst.id.clone())),
        })
    }

    /// The footer: only keys that work right now.
    pub fn hints(&self, data: &Data) -> Vec<Hint> {
        let key = Hint::key;
        if data.instances.is_empty() {
            return vec![
                key(KeyCode::Enter, "new instance"),
                key(KeyCode::Char('a'), "sign in"),
                key(KeyCode::Char('s'), "settings"),
                key(KeyCode::Char('?'), "help"),
                key(KeyCode::Char('q'), "quit"),
            ];
        }
        match (&self.filter, self.focus) {
            (Filter::Editing(_), _) => vec![
                key(KeyCode::Enter, "apply filter"),
                key(KeyCode::Esc, "cancel"),
            ],
            (_, Pane::Detail) => vec![
                Hint::show("←→", "tabs"),
                key(
                    KeyCode::Enter,
                    if self.tab == Tab::Overview {
                        "play"
                    } else {
                        "open"
                    },
                ),
                key(KeyCode::Esc, "list"),
                key(KeyCode::Char(':'), "commands"),
                key(KeyCode::Char('?'), "help"),
            ],
            (Filter::Applied(_), Pane::List) => {
                let enter = self
                    .selected_instance(data)
                    .map(|inst| key(KeyCode::Enter, Self::play_label(inst, data)));
                enter
                    .into_iter()
                    .chain([
                        key(KeyCode::Esc, "clear filter"),
                        key(KeyCode::Char('/'), "edit filter"),
                        key(KeyCode::Char('?'), "help"),
                    ])
                    .collect()
            }
            (Filter::Off, Pane::List) => match self.selected_instance(data) {
                None => vec![
                    key(KeyCode::Enter, "new instance"),
                    key(KeyCode::Char('/'), "search"),
                    key(KeyCode::Char(':'), "commands"),
                    key(KeyCode::Char('?'), "help"),
                ],
                Some(inst) => vec![
                    key(KeyCode::Enter, Self::play_label(inst, data)),
                    key(KeyCode::Char('o'), "offline"),
                    key(KeyCode::Char('n'), "new"),
                    key(KeyCode::Char('/'), "search"),
                    key(KeyCode::Char(':'), "commands"),
                    key(KeyCode::Char('?'), "help"),
                ],
            },
        }
    }

    /// What Enter does to an instance, in words.
    fn play_label(inst: &Instance, data: &Data) -> &'static str {
        if play::is_playing(data, &inst.id) {
            "view game"
        } else if play::unseen_trouble(data, &inst.id).is_some() {
            "see what happened"
        } else if crate::accounts::needs_sign_in(data) {
            "sign in & play"
        } else {
            "play"
        }
    }

    fn help_groups() -> Vec<HelpGroup> {
        vec![
            HelpGroup {
                title: "Navigate",
                entries: &[
                    ("↑↓ j k", "move"),
                    ("pgup pgdn", "page"),
                    ("tab", "list ↔ details"),
                    ("→ l ]", "details · next tab"),
                    ("← h [", "back · previous tab"),
                ],
            },
            HelpGroup {
                title: "Act",
                entries: &[
                    ("↵", "play"),
                    ("o", "play offline"),
                    ("n", "new instance"),
                    ("e", "edit instance"),
                    ("d ⌫", "delete instance"),
                    ("f", "open folder"),
                ],
            },
            HelpGroup {
                title: "Screens",
                entries: &[
                    ("m", "mods"),
                    ("p", "resource packs"),
                    ("s", "settings"),
                    ("a", "accounts"),
                ],
            },
            HelpGroup {
                title: "General",
                entries: &[
                    ("x", "actions"),
                    (":", "command palette"),
                    ("/", "filter"),
                    ("esc", "clear filter · back"),
                    ("?", "help"),
                    ("q", "quit"),
                    ("double-click", "play"),
                    ("right-click", "actions"),
                ],
            },
        ]
    }

    /// Commands for the `:` palette: verbs, then a jump row per instance.
    pub fn palette_items(&self, data: &Data) -> Vec<PaletteItem> {
        let verb = |label: &str, key, action| PaletteItem {
            label: label.to_owned(),
            key: Some(key),
            jump: false,
            action,
        };
        let mut items = Vec::new();
        if let Some(inst) = self.selected_instance(data) {
            let play = if play::is_playing(data, &inst.id) {
                "View game"
            } else if play::unseen_trouble(data, &inst.id).is_some() {
                "See what happened"
            } else {
                "Play"
            };
            items.extend([
                verb(play, "↵", Action::launch(inst, false)),
                verb("Play offline", "o", Action::launch(inst, true)),
                verb("Mods", "m", mods(inst)),
                verb(
                    "Resource packs",
                    "p",
                    Action::Open(Route::ResourcePacks(inst.id.clone())),
                ),
                verb(
                    "Edit instance",
                    "e",
                    Action::Open(Route::EditInstance(inst.id.clone())),
                ),
                verb(
                    "Delete instance",
                    "d",
                    Action::ConfirmDelete(inst.id.clone()),
                ),
                verb("Open folder", "f", Action::OpenFolder(inst.id.clone())),
            ]);
        }
        items.extend([
            verb("New instance", "n", Action::Open(Route::NewInstance)),
            verb("Settings", "s", Action::Open(Route::Settings)),
            verb("Accounts", "a", Action::Open(Route::Accounts)),
        ]);
        items.extend(data.instances.iter().map(|inst| PaletteItem {
            label: inst.name.clone(),
            key: None,
            jump: true,
            action: Action::Select(inst.id.clone()),
        }));
        items
    }

    /// Jumps to an instance, clearing any filter that hides it.
    pub fn select_id(&mut self, id: &str, data: &Data) {
        if !matches!(self.filter, Filter::Off) {
            self.filter = Filter::Off;
            self.refresh(data);
        }
        let moved = self.list.select_key(id);
        self.after_move(moved, data);
    }

    /// The `x` card / right-click menu for an instance.
    pub fn menu_items(inst: &Instance) -> Vec<MenuItem> {
        let item = |label, key, action| MenuItem {
            label,
            key: KeyCode::Char(key),
            danger: false,
            action,
        };
        let mut items = vec![
            MenuItem {
                label: "Play",
                key: KeyCode::Enter,
                danger: false,
                action: Action::launch(inst, false),
            },
            item("Play offline", 'o', Action::launch(inst, true)),
        ];
        if inst.loader.supports_mod_browser() {
            items.push(item(
                "Mods",
                'm',
                Action::Open(Route::Mods(inst.id.clone())),
            ));
        }
        items.extend([
            item(
                "Resource packs",
                'p',
                Action::Open(Route::ResourcePacks(inst.id.clone())),
            ),
            item(
                "Edit",
                'e',
                Action::Open(Route::EditInstance(inst.id.clone())),
            ),
            item("Open folder", 'f', Action::OpenFolder(inst.id.clone())),
            MenuItem {
                danger: true,
                ..item("Delete", 'd', Action::ConfirmDelete(inst.id.clone()))
            },
        ]);
        items
    }
}

impl Screen for Home {
    fn crumbs(&self, _data: &Data) -> Vec<String> {
        Vec::new()
    }

    fn hints(&self, data: &Data) -> Vec<Hint> {
        Home::hints(self, data)
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        ("Home", Self::help_groups())
    }

    fn palette(&self, data: &Data) -> Vec<PaletteItem> {
        self.palette_items(data)
    }

    fn captures_text(&self) -> bool {
        Home::captures_text(self)
    }

    fn set_body(&mut self, body: Rect) {
        Home::set_body(self, body);
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        self.on_key(key, cx.data)
    }

    fn handle_mouse(&mut self, target: &ui::Target, mouse: Mouse, cx: &mut Ctx<'_>) -> Handled {
        if let Some(key) = self.list.key_of(target) {
            self.on_row(key, mouse, cx.data)
        } else if let ui::Target::Home(target) = target {
            self.on_mouse(target, mouse, cx.data)
        } else {
            Handled::Ignored
        }
    }

    fn handle_paste(&mut self, text: &str, cx: &mut Ctx<'_>) -> Handled {
        Handled::changed(self.on_paste(text, cx.data))
    }

    fn reload(&mut self, data: &Data) {
        Home::reload(self, data);
    }

    fn render(&self, cv: &mut Canvas<'_>, data: &Data) {
        self.draw(cv, data);
    }
}

/// The letter keys that act on the selected instance.
fn instance_action(inst: &Instance, code: KeyCode) -> Option<Action> {
    let id = || inst.id.clone();
    Some(match code {
        KeyCode::Char('o') => Action::launch(inst, true),
        KeyCode::Char('m') => mods(inst),
        KeyCode::Char('p') => Action::Open(Route::ResourcePacks(id())),
        KeyCode::Char('e') => Action::Open(Route::EditInstance(id())),
        KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => Action::ConfirmDelete(id()),
        KeyCode::Char('f') => Action::OpenFolder(id()),
        KeyCode::Char('x') => Action::ActionsMenu {
            id: id(),
            anchor: None,
        },
        _ => return None,
    })
}

/// The mods screen, or why it isn't available for this instance.
fn mods(inst: &Instance) -> Action {
    if inst.loader.supports_mod_browser() {
        Action::Open(Route::Mods(inst.id.clone()))
    } else {
        Action::Banner(Banner::info(
            "The mods browser works with Fabric instances.",
        ))
    }
}

#[cfg(test)]
mod tests;
