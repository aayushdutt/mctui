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
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::ui::input::TextInput;
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

/// A list row by identity, so a click or a selection survives the list
/// being rebuilt (a reload, a filter) underneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKey {
    Instance(String),
    New,
}

/// Home's clickable parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Row(RowKey),
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
    fn key(&self) -> RowKey {
        match self {
            Row::Instance { id, .. } => RowKey::Instance(id.clone()),
            Row::New => RowKey::New,
        }
    }

    fn is(&self, key: &RowKey) -> bool {
        match (self, key) {
            (Row::Instance { id, .. }, RowKey::Instance(k)) => id == k,
            (Row::New, RowKey::New) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Default)]
enum Filter {
    #[default]
    Off,
    /// Typing a query; keys go to the field.
    Editing(TextInput),
    /// A query narrows the list; keys work as usual.
    Applied(TextInput),
}

impl Filter {
    fn query(&self) -> &str {
        match self {
            Filter::Off => "",
            Filter::Editing(q) | Filter::Applied(q) => q.as_str(),
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
    /// Index into `rows`.
    selected: usize,
    /// First row drawn.
    offset: usize,
    focus: Pane,
    tab: Tab,
    filter: Filter,
    facets: HashMap<String, Facets>,
    body: Rect,
    /// The instance whose game is running.
    playing: Option<String>,
    /// The empty screen's wink, picked once per run.
    splash: &'static str,
}

impl Home {
    pub fn new(data: &Data, splash: &'static str) -> Self {
        let mut home = Self {
            rows: Vec::new(),
            selected: 0,
            offset: 0,
            focus: Pane::List,
            tab: Tab::Overview,
            filter: Filter::Off,
            facets: HashMap::new(),
            body: Rect::default(),
            playing: None,
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
        // Read the selection from the rows, not through `data`: the rows'
        // indices belong to the data they were built from, which may be gone.
        let before = self.rows.get(self.selected).map(Row::key);
        let on_new = before == Some(RowKey::New) && !data.instances.is_empty();

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

        self.selected = before
            .and_then(|key| self.row_index(&key))
            .or_else(|| on_new.then(|| self.rows.len().saturating_sub(1)))
            .unwrap_or(0);
        self.after_select(data);
    }

    fn row_index(&self, key: &RowKey) -> Option<usize> {
        self.rows.iter().position(|r| r.is(key))
    }

    /// The body area changed (resize, a banner appeared).
    pub fn set_body(&mut self, body: Rect) {
        self.body = body;
        if layout::layout(body).compact() {
            self.focus = Pane::List;
        }
        self.scroll_into_view();
    }

    /// Whether keys are going into a text field (so `?`, `:` and `q` type).
    pub fn captures_text(&self) -> bool {
        matches!(self.filter, Filter::Editing(_))
    }

    pub fn selected_instance<'d>(&self, data: &'d Data) -> Option<&'d Instance> {
        match self.rows.get(self.selected)? {
            Row::Instance { index, .. } => data.instances.get(*index),
            Row::New => None,
        }
    }

    /// Moves the selection to `row`. Picking another instance returns focus to
    /// the list, so the detail pane never shows stale focus.
    fn select(&mut self, row: usize, data: &Data) -> bool {
        let row = row.min(self.rows.len().saturating_sub(1));
        if row == self.selected {
            return false;
        }
        self.selected = row;
        self.focus = Pane::List;
        self.tab = Tab::Overview;
        self.after_select(data);
        true
    }

    fn after_select(&mut self, data: &Data) {
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
        self.scroll_into_view();
    }

    /// How many rows fit in the list at once.
    fn visible_rows(&self) -> usize {
        let height = layout::list_rows(&layout::layout(self.body)).height;
        usize::from(height.div_ceil(view::ROW_PITCH)).max(1)
    }

    /// Adjusts the scroll so the selected row is visible, without leaving
    /// empty space below the last row.
    fn scroll_into_view(&mut self) {
        let visible = self.visible_rows();
        let max_offset = self.rows.len().saturating_sub(visible);
        self.offset = self
            .offset
            .max((self.selected + 1).saturating_sub(visible))
            .min(self.selected)
            .min(max_offset);
    }

    fn step(&mut self, delta: isize, data: &Data) -> bool {
        let row = self.selected.saturating_add_signed(delta);
        self.select(row, data)
    }

    fn page(&self) -> isize {
        isize::try_from(self.visible_rows()).unwrap_or(1)
    }

    fn on_paste(&mut self, text: &str, data: &Data) -> bool {
        match &mut self.filter {
            Filter::Editing(input) => {
                if input.insert(text) {
                    self.refresh(data);
                }
                true
            }
            _ => false,
        }
    }

    fn on_key(&mut self, key: KeyEvent, data: &Data) -> Handled {
        if matches!(self.filter, Filter::Editing(_)) {
            return self.filter_key(key, data);
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
        if let Some(handled) = self.navigate(key.code, data) {
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
                    self.filter = Filter::Editing(TextInput::new(self.filter.query()));
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

    /// Keys while typing a filter: they edit it, except Enter (keep it), Esc
    /// (drop it) and ↑↓ (move through the matches).
    fn filter_key(&mut self, key: KeyEvent, data: &Data) -> Handled {
        let Filter::Editing(input) = &mut self.filter else {
            return Handled::Ignored;
        };
        match key.code {
            KeyCode::Esc => self.filter = Filter::Off,
            KeyCode::Enter => {
                self.commit_filter(data);
                return Handled::Consumed;
            }
            KeyCode::Up => return moved(self.step(-1, data)),
            KeyCode::Down => return moved(self.step(1, data)),
            _ => {
                if input.handle_key(key) != Some(true) {
                    return Handled::Consumed;
                }
            }
        }
        self.refresh(data);
        Handled::Consumed
    }

    /// Moving between and within panes.
    fn navigate(&mut self, code: KeyCode, data: &Data) -> Option<Handled> {
        let split = !layout::layout(self.body).compact();
        let handled = match code {
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
            KeyCode::Up | KeyCode::Char('k') => moved(self.step(-1, data)),
            KeyCode::Down | KeyCode::Char('j') => moved(self.step(1, data)),
            KeyCode::PageUp => moved(self.step(-self.page(), data)),
            KeyCode::PageDown => moved(self.step(self.page(), data)),
            KeyCode::Home | KeyCode::Char('g') => moved(self.select(0, data)),
            KeyCode::End | KeyCode::Char('G') => moved(self.select(usize::MAX, data)),
            _ => return None,
        };
        Some(handled)
    }

    /// Stops typing into the filter, keeping what was typed.
    pub fn commit_filter(&mut self, data: &Data) {
        if let Filter::Editing(input) = &mut self.filter {
            let query = std::mem::take(input);
            self.filter = if query.as_str().trim().is_empty() {
                Filter::Off
            } else {
                Filter::Applied(query)
            };
            self.refresh(data);
        }
    }

    fn on_mouse(&mut self, target: &Target, mouse: Mouse, data: &Data) -> Handled {
        if !matches!(mouse, Mouse::Scroll { .. }) {
            self.commit_filter(data);
        }
        match (target, mouse) {
            (Target::Row(_) | Target::List, Mouse::Scroll { down }) => {
                moved(self.step(if down { 1 } else { -1 }, data))
            }
            (Target::Row(key), Mouse::Click { double }) => {
                let Some(row) = self.row_index(key) else {
                    return Handled::Ignored;
                };
                self.select(row, data);
                self.focus_list();
                if double {
                    return match self.selected_instance(data) {
                        Some(inst) => Handled::Action(Action::launch(inst, false)),
                        None => Handled::Action(Action::Open(Route::NewInstance)),
                    };
                }
                Handled::Consumed
            }
            (Target::Row(key), Mouse::RightClick(at)) => {
                let Some(row) = self.row_index(key) else {
                    return Handled::Ignored;
                };
                self.select(row, data);
                self.focus_list();
                match self.selected_instance(data) {
                    Some(inst) => Handled::Action(Action::ActionsMenu {
                        id: inst.id.clone(),
                        anchor: Some(at),
                    }),
                    None => Handled::Consumed,
                }
            }
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
            (Filter::Applied(_), Pane::List) => vec![
                key(KeyCode::Enter, "play"),
                key(KeyCode::Esc, "clear filter"),
                key(KeyCode::Char('/'), "edit filter"),
                key(KeyCode::Char('?'), "help"),
            ],
            (Filter::Off, Pane::List) => match self.selected_instance(data) {
                None => vec![
                    key(KeyCode::Enter, "new instance"),
                    key(KeyCode::Char('/'), "search"),
                    key(KeyCode::Char(':'), "commands"),
                    key(KeyCode::Char('?'), "help"),
                ],
                Some(inst) => vec![
                    key(KeyCode::Enter, self.play_label(inst, data)),
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
    fn play_label(&self, inst: &Instance, data: &Data) -> &'static str {
        if self.playing.as_deref() == Some(inst.id.as_str()) {
            "view game"
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
                    ("d", "delete instance"),
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
            let play = if self.playing.as_deref() == Some(inst.id.as_str()) {
                "View game"
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
        if let Some(row) = self.row_index(&RowKey::Instance(id.to_owned())) {
            self.select(row, data);
        }
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
        match target {
            ui::Target::Home(target) => self.on_mouse(target, mouse, cx.data),
            _ => Handled::Ignored,
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

/// A movement key did something only if the selection moved.
fn moved(changed: bool) -> Handled {
    Handled::changed(changed)
}

/// The letter keys that act on the selected instance.
fn instance_action(inst: &Instance, code: KeyCode) -> Option<Action> {
    let id = || inst.id.clone();
    Some(match code {
        KeyCode::Char('o') => Action::launch(inst, true),
        KeyCode::Char('m') => mods(inst),
        KeyCode::Char('p') => Action::Open(Route::ResourcePacks(id())),
        KeyCode::Char('e') => Action::Open(Route::EditInstance(id())),
        KeyCode::Char('d') | KeyCode::Delete => Action::ConfirmDelete(id()),
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
