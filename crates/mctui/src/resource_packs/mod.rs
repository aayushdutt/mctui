//! Resource packs: pick Vanilla Tweaks for an instance and build them into
//! one pack the game loads.
//!
//! Three panes — the category tree, the selected category's packs, and your
//! pack — sit side by side on wide terminals; narrower, the packs and your
//! pack take turns beside the categories, and narrower still every pane gets
//! the screen to itself. Picks save as you make them; `b` has the site build
//! them into one zip, downloads it, and switches it on in the game.

mod browser;
mod layout;
#[cfg(test)]
pub(crate) mod tests;
mod view;

use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mctui_core::Loaded;
use mctui_core::instance::Instance;
use mctui_core::net;
use mctui_core::vanilla_tweaks::{
    self, Cart, Catalog, CatalogVersion, PackState, Stage, Toggled, Unsupported,
};
use ratatui::layout::{Position, Rect};

use self::browser::{Browser, Job, Saver, Work};
use crate::app::{Action, Banner, Question};
use crate::data::Data;
use crate::format;
use crate::overlay::{HelpGroup, MenuItem, PaletteItem};
use crate::screen::{self, Ctx, Handled, Mouse, Screen};
use crate::services::Services;
use crate::task::{Task, Tasks};
use crate::ui::{self, Canvas, Hint, Target, widgets};

/// Which pane has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Categories,
    Packs,
    Cart,
}

impl Pane {
    const ALL: [Pane; 3] = [Pane::Categories, Pane::Packs, Pane::Cart];

    fn key(self) -> &'static str {
        match self {
            Pane::Categories => "categories",
            Pane::Packs => "packs",
            Pane::Cart => "cart",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|pane| pane.key() == key)
    }

    /// The next pane over, wrapping.
    fn cycle(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|p| *p == self).unwrap_or(0);
        let n = Self::ALL.len();
        Self::ALL[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

/// Click targets that aren't list rows, as [`Target::Row`]s of these "lists":
/// a pane's empty space, the tabs that switch panes, and buttons.
const PANE: &str = "packs.pane";
const TAB: &str = "packs.tab";
const BUTTON: &str = "packs.button";

fn target(kind: &'static str, key: &str) -> Target {
    Target::Row {
        list: kind,
        key: key.to_owned(),
    }
}

/// What the screen's background work reports.
#[derive(Debug)]
pub enum Msg {
    /// The catalog, the cart, and whether the pack is on in the game.
    Loaded(Box<Load>),
    /// The catalog again, after starting offline.
    Refreshed(Result<Loaded<Catalog>, vanilla_tweaks::Error>),
    /// Job `seq`'s build moved on to its next stage.
    Stage(u64, Stage),
    /// Job `seq`'s build finished: the cart as applied.
    Applied(u64, Result<Cart, vanilla_tweaks::Error>),
    /// Job `seq` took the pack out of the game: the cart without a build.
    Removed(u64, Result<Cart, vanilla_tweaks::Error>),
    SaveFailed(String),
}

/// Everything the screen needs to open.
#[derive(Debug)]
pub struct Load {
    catalog: Result<Loaded<Catalog>, vanilla_tweaks::Error>,
    cart: Loaded<Cart>,
    enabled: bool,
}

fn app_msg(msg: Msg) -> crate::msg::Msg {
    crate::msg::Msg::ResourcePacks(msg)
}

/// What keys, clicks, menu entries and palette rows do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Preview,
    Video,
    Search,
    Build,
    /// Asks, then [`Command::ClearNow`].
    Clear,
    ClearNow,
    /// Asks, then [`Command::RemoveNow`].
    Remove,
    RemoveNow,
    Retry,
}

impl Command {
    const ALL: [Command; 10] = [
        Command::Toggle,
        Command::Preview,
        Command::Video,
        Command::Search,
        Command::Build,
        Command::Clear,
        Command::ClearNow,
        Command::Remove,
        Command::RemoveNow,
        Command::Retry,
    ];

    fn name(self) -> &'static str {
        match self {
            Command::Toggle => "toggle",
            Command::Preview => "preview",
            Command::Video => "video",
            Command::Search => "search",
            Command::Build => "build",
            Command::Clear => "clear",
            Command::ClearNow => "clear-now",
            Command::Remove => "remove",
            Command::RemoveNow => "remove-now",
            Command::Retry => "retry",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    fn action(self) -> Action {
        Action::Command(screen::Command::ResourcePacks(self))
    }

    fn button(self) -> Target {
        target(BUTTON, self.name())
    }
}

/// The instance whose packs these are.
#[derive(Debug)]
struct Inst {
    name: String,
    version: String,
    game_dir: PathBuf,
}

#[derive(Debug)]
enum State {
    /// Vanilla Tweaks has no packs for the instance's version.
    Unsupported(Unsupported),
    Loading {
        since: Instant,
        _task: Task,
    },
    /// The catalog couldn't be loaded; says why and what to do.
    Failed(String),
    Ready(Box<Browser>),
}

#[derive(Debug)]
pub struct ResourcePacks {
    inst: Inst,
    state: State,
    body: Rect,
    /// The spinner's clock: when it last advanced.
    frame: Instant,
}

impl ResourcePacks {
    /// Opens the screen for `inst` and starts loading its packs.
    pub fn new(inst: &Instance, services: &Services, tasks: &Tasks) -> Self {
        let inst = Inst {
            name: inst.name.clone(),
            version: inst.version.clone(),
            game_dir: inst.game_dir(),
        };
        let state = load(&inst, services, tasks);
        Self {
            inst,
            state,
            body: Rect::default(),
            frame: Instant::now(),
        }
    }

    /// Sizes the lists for the current layout.
    fn fit(&mut self) {
        let State::Ready(b) = &mut self.state else {
            return;
        };
        let lay = layout::layout(self.body, b.focus, b.offline);
        if let Some(pane) = lay.categories {
            b.categories.set_height(layout::content(pane).height);
        }
        if let Some(pane) = lay.packs {
            let area = layout::packs_area(pane, view::warning_lines(b, pane.width));
            b.packs.set_height(area.list.height);
            b.results.set_height(area.list.height);
        }
        if let Some(pane) = lay.cart {
            let footer = view::cart_footer_height(b, pane.width);
            b.basket
                .set_height(layout::cart_area(pane, footer).0.height);
        }
    }

    fn on_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        let command = match &mut self.state {
            State::Ready(b) if b.search.as_ref().is_some_and(|s| s.typing) => {
                return search_key(b, key);
            }
            _ if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                return Handled::Ignored;
            }
            State::Ready(b) => match browse_key(b, key) {
                Ok(handled) => return handled,
                Err(command) => command,
            },
            State::Failed(_) if key.code == KeyCode::Char('r') => Command::Retry,
            _ => return Handled::Ignored,
        };
        self.run(command, cx)
    }

    fn run(&mut self, command: Command, cx: &mut Ctx<'_>) -> Handled {
        if let State::Failed(_) = self.state {
            if command != Command::Retry {
                return Handled::Ignored;
            }
            self.state = load(&self.inst, cx.services, cx.tasks);
            return Handled::Consumed;
        }
        let State::Ready(b) = &mut self.state else {
            return Handled::Ignored;
        };
        let inst = &self.inst;
        match command {
            Command::Toggle => toggle(b),
            Command::Preview => preview(b, cx.services),
            Command::Video => video(b),
            Command::Search => {
                b.start_search();
                Handled::Consumed
            }
            Command::Build => build(b, inst, cx),
            Command::Clear => ask_clear(b),
            Command::ClearNow => {
                b.clear();
                Handled::Consumed
            }
            Command::Remove => ask_remove(b, inst),
            Command::RemoveNow => remove(b, inst, cx),
            Command::Retry => retry(b, cx),
        }
    }

    fn on_mouse(&mut self, target: &Target, mouse: Mouse, cx: &mut Ctx<'_>) -> Handled {
        let Target::Row { list, key } = target else {
            return Handled::Ignored;
        };
        if *list == BUTTON {
            return match (Command::parse(key), mouse) {
                (Some(command), Mouse::Click { double: false }) => self.run(command, cx),
                _ => Handled::Ignored,
            };
        }
        let State::Ready(b) = &mut self.state else {
            return Handled::Ignored;
        };
        let pane = match *list {
            PANE | TAB => Pane::from_key(key),
            browser::CATEGORIES => Some(Pane::Categories),
            browser::PACKS | browser::RESULTS => Some(Pane::Packs),
            browser::CART => Some(Pane::Cart),
            _ => None,
        };
        let Some(pane) = pane else {
            return Handled::Ignored;
        };
        match mouse {
            Mouse::Scroll { down } => {
                b.focus(pane);
                b.step(if down { 1 } else { -1 });
                Handled::Consumed
            }
            _ if matches!(*list, PANE | TAB) => match mouse {
                Mouse::Click { double: false } => Handled::changed(b.focus(pane)),
                _ => Handled::Ignored,
            },
            Mouse::Click { double } => {
                b.focus(pane);
                match pane {
                    Pane::Categories => click_category(b, key, double),
                    Pane::Packs | Pane::Cart => {
                        select_pack(b, key);
                        if double { toggle(b) } else { Handled::Consumed }
                    }
                }
            }
            Mouse::RightClick(at) => {
                b.focus(pane);
                match pane {
                    Pane::Categories => {
                        b.select_category(key);
                        Handled::Consumed
                    }
                    Pane::Packs | Pane::Cart => {
                        select_pack(b, key);
                        menu(b, at)
                    }
                }
            }
        }
    }

    fn loaded(&mut self, load: Load, cx: &mut Ctx<'_>) -> Handled {
        let Load {
            catalog,
            cart,
            enabled,
        } = load;
        let catalog = match catalog {
            Ok(catalog) => catalog,
            Err(e) => {
                self.state = State::Failed(describe(&e, 'r'));
                return Handled::Consumed;
            }
        };
        if let Some(notice) = &catalog.notice {
            tracing::info!(notice, "resource packs are offline");
        }
        let saver = Saver::start(cx.tasks, self.inst.game_dir.clone(), cart.value.clone());
        let offline = catalog.notice.is_some();
        let browser = Browser::new(catalog.value, offline, cart.value, enabled, saver);
        self.state = State::Ready(Box::new(browser));
        self.fit();
        match cart.notice {
            Some(notice) => Handled::Action(Action::Banner(Banner::warn(notice))),
            None => Handled::Consumed,
        }
    }

    fn on_msg(&mut self, msg: Msg, cx: &mut Ctx<'_>) -> Handled {
        if let Msg::Loaded(load) = msg {
            return self.loaded(*load, cx);
        }
        let State::Ready(b) = &mut self.state else {
            return Handled::Ignored;
        };
        let banner = |banner| Handled::Action(Action::Banner(banner));
        // A result from a job that was stopped (or replaced) since.
        if let Msg::Stage(seq, _) | Msg::Applied(seq, _) | Msg::Removed(seq, _) = &msg
            && b.job.as_ref().is_none_or(|job| job.seq != *seq)
        {
            return Handled::Ignored;
        }
        match msg {
            Msg::Loaded(_) => Handled::Ignored,
            Msg::SaveFailed(error) => {
                banner(Banner::warn(format!("Couldn't save your picks: {error}")))
            }
            Msg::Refreshed(result) => {
                b.retry = None;
                match result {
                    Ok(catalog) if catalog.notice.is_none() => {
                        b.set_catalog(catalog.value, false);
                        banner(Banner::success("Back online: these are the latest packs."))
                    }
                    Ok(_) | Err(_) => banner(Banner::warn(
                        "Still can't reach Vanilla Tweaks — showing the packs from your last visit.",
                    )),
                }
            }
            Msg::Stage(_, stage) => match (&mut b.job, stage) {
                (Some(job), Stage::Downloading) if job.work == Work::Building => {
                    job.work = Work::Downloading;
                    Handled::Consumed
                }
                _ => Handled::Ignored,
            },
            Msg::Applied(_, result) => {
                b.job = None;
                match result {
                    Ok(cart) if cart.is_empty() => {
                        b.set_cart(cart, false);
                        banner(Banner::success(format!(
                            "Removed your pack from “{}”.",
                            self.inst.name
                        )))
                    }
                    Ok(cart) => {
                        let applied = format!("Applied {}.", format::count(cart.len(), "tweak"));
                        b.set_cart(cart, true);
                        banner(Banner::success(applied))
                    }
                    Err(e) => banner(Banner::warn(describe(&e, 'b'))),
                }
            }
            Msg::Removed(_, result) => {
                b.job = None;
                match result {
                    Ok(cart) => {
                        b.set_cart(cart, false);
                        banner(Banner::success(format!(
                            "Removed your pack from “{}”. Your picks are still here.",
                            self.inst.name
                        )))
                    }
                    Err(e) => banner(Banner::warn(describe(&e, 'u'))),
                }
            }
        }
    }

    fn spinning_since(&self) -> Option<Instant> {
        match &self.state {
            State::Loading { since, .. } => Some(*since),
            State::Ready(b) => b
                .job
                .as_ref()
                .map(|job| job.since)
                .or(b.retry.as_ref().map(|(since, _)| *since)),
            State::Unsupported(_) | State::Failed(_) => None,
        }
    }
}

/// Starts loading the catalog, the cart, and whether the pack is on.
fn load(inst: &Inst, services: &Services, tasks: &Tasks) -> State {
    let version = match CatalogVersion::for_minecraft(&inst.version) {
        Ok(version) => version,
        Err(why) => return State::Unsupported(why),
    };
    let site = services.vanilla_tweaks.clone();
    let game = inst.game_dir.clone();
    let work = async move {
        let (catalog, cart, enabled) = tokio::join!(
            site.catalog(version),
            Cart::load(&game),
            vanilla_tweaks::is_enabled(&game, version),
        );
        Load {
            catalog,
            cart,
            enabled,
        }
    };
    State::Loading {
        since: Instant::now(),
        _task: tasks.run(work, |load| app_msg(Msg::Loaded(Box::new(load)))),
    }
}

/// Keys while typing a search: they edit it, except ↵ (keep the results), esc
/// (leave the search), tab and the arrows (move through the results).
fn search_key(b: &mut Browser, key: KeyEvent) -> Handled {
    match key.code {
        KeyCode::Esc => {
            b.end_search();
        }
        KeyCode::Enter => {
            b.stop_typing();
        }
        KeyCode::Tab | KeyCode::BackTab => {
            b.focus(b.focus.cycle(key.code == KeyCode::Tab));
        }
        KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
            return Handled::changed(b.results.handle_key(key) == Some(true));
        }
        _ => {
            let edited = b
                .search
                .as_mut()
                .is_some_and(|search| search.input.handle_key(key) == Some(true));
            if edited {
                b.searched();
            }
        }
    }
    Handled::Consumed
}

/// Keys while browsing. Returns the command to run for keys that are one.
fn browse_key(b: &mut Browser, key: KeyEvent) -> Result<Handled, Command> {
    let code = key.code;
    let handled = match code {
        KeyCode::Char('/') => return Err(Command::Search),
        KeyCode::Char('b') => return Err(Command::Build),
        KeyCode::Char('c') => return Err(Command::Clear),
        KeyCode::Char('u') => return Err(Command::Remove),
        KeyCode::Char('o') => return Err(Command::Preview),
        KeyCode::Char('w') => return Err(Command::Video),
        KeyCode::Char('r') => return Err(Command::Retry),
        KeyCode::Char(' ') if b.focus != Pane::Categories => return Err(Command::Toggle),
        KeyCode::Enter if b.focus == Pane::Packs => return Err(Command::Toggle),
        // ↵ never removes: in your pack it shows the pack in its category.
        KeyCode::Enter if b.focus == Pane::Cart => match b.focused_pack() {
            Some(row) => {
                let id = row.id.clone();
                b.reveal(&id)
            }
            None => false,
        },
        KeyCode::Enter | KeyCode::Char(' ') => match b.selected_node().cloned() {
            Some(node) if node.expandable => b.set_expanded(&node.key, !node.expanded),
            Some(_) => b.focus(Pane::Packs),
            None => false,
        },
        KeyCode::Esc => {
            if b.end_search() {
                true
            } else if let Some(job) = b.job.take() {
                drop(job);
                return Ok(Handled::Action(Action::Banner(Banner::info(
                    "Stopped. Your pack in the game is as it was.",
                ))));
            } else {
                // Nothing to step back from: the app closes the screen.
                return Ok(Handled::Ignored);
            }
        }
        KeyCode::Tab | KeyCode::BackTab => b.focus(b.focus.cycle(code == KeyCode::Tab)),
        KeyCode::Right | KeyCode::Char('l') => b.right(),
        KeyCode::Left | KeyCode::Char('h') => b.left(),
        _ => {
            let down = matches!(
                code,
                KeyCode::Down | KeyCode::Char('j' | 'G') | KeyCode::PageDown | KeyCode::End
            );
            let list = b.focused_list();
            let before = list.selected();
            if list.handle_key(key).is_none() {
                return Ok(Handled::Ignored);
            }
            b.after_move(down);
            b.focused_list().selected() != before
        }
    };
    Ok(Handled::changed(handled))
}

/// A click on a category row: selects it, and opens or closes its
/// subcategories. A double-click moves on to its packs.
fn click_category(b: &mut Browser, key: &str, double: bool) -> Handled {
    b.select_category(key);
    match b.selected_node().cloned() {
        Some(_) if double => {
            b.focus(Pane::Packs);
        }
        Some(node) if node.expandable => {
            b.set_expanded(&node.key, !node.expanded);
        }
        _ => {}
    }
    Handled::Consumed
}

/// Selects the pack row `key` in the focused pane (headings step aside).
fn select_pack(b: &mut Browser, key: &str) {
    b.focused_list().select_key(key);
    b.after_move(true);
}

/// Edits wait while the pack is being built or removed.
fn busy(b: &Browser) -> Option<Handled> {
    b.job.as_ref().map(|_| {
        Handled::Action(Action::Banner(Banner::info(
            "Wait for your pack to finish, or press esc to stop it.",
        )))
    })
}

fn toggle(b: &mut Browser) -> Handled {
    let Some(row) = b.focused_pack().cloned() else {
        return Handled::Ignored;
    };
    if let Some(wait) = busy(b) {
        return wait;
    }
    match b.toggle(&row.category, &row.id) {
        Toggled::Added { replaced } if !replaced.is_empty() => {
            let names: Vec<&str> = replaced.iter().map(|p| b.label(&p.name)).collect();
            Handled::Action(Action::Banner(Banner::info(format!(
                "Swapped in {}, replacing {}.",
                row.label,
                and_list(&names)
            ))))
        }
        Toggled::Added { .. } | Toggled::Removed => Handled::Consumed,
    }
}

fn preview(b: &Browser, services: &Services) -> Handled {
    let Some((_, pack)) = b.focused_pack().and_then(|row| b.pack(&row.id)) else {
        return Handled::Ignored;
    };
    let url = services.vanilla_tweaks.preview_url(b.catalog.version, pack);
    open(&url, format!("Opened the preview of {}.", pack.label()))
}

fn video(b: &Browser) -> Handled {
    let Some((_, pack)) = b.focused_pack().and_then(|row| b.pack(&row.id)) else {
        return Handled::Ignored;
    };
    match &pack.video {
        Some(url) => open(url, format!("Opened the video for {}.", pack.label())),
        None => Handled::Ignored,
    }
}

/// Opens `url` in the browser, saying `done`; if the browser won't open, the
/// app says so instead.
fn open(url: &str, done: String) -> Handled {
    Handled::Action(Action::Many(vec![
        Action::Banner(Banner::info(done)),
        Action::OpenUrl(url.to_owned()),
    ]))
}

fn build(b: &mut Browser, inst: &Inst, cx: &Ctx<'_>) -> Handled {
    if b.job.is_some() {
        return Handled::Ignored;
    }
    if b.cart.is_empty() && !b.is_built() {
        return Handled::Action(Action::Banner(Banner::info(
            "Pick some tweaks first: space adds the selected pack.",
        )));
    }
    let site = cx.services.vanilla_tweaks.clone();
    let game = inst.game_dir.clone();
    let version = b.catalog.version;
    let mut cart = b.cart.clone();
    let work = if cart.is_empty() {
        Work::Removing
    } else {
        Work::Building
    };
    b.jobs += 1;
    let seq = b.jobs;
    let task = cx.tasks.spawn(move |mail| async move {
        let progress = mail.clone();
        let applied = site
            .apply_with(&game, version, &mut cart, move |stage| {
                progress.send(app_msg(Msg::Stage(seq, stage)));
            })
            .await;
        mail.send(app_msg(Msg::Applied(seq, applied.map(|()| cart))));
    });
    b.job = Some(Job::new(seq, work, cx.now, task));
    Handled::Consumed
}

fn ask_clear(b: &Browser) -> Handled {
    if b.cart.is_empty() {
        return Handled::Ignored;
    }
    if let Some(wait) = busy(b) {
        return wait;
    }
    Handled::Action(Action::Confirm(Box::new(Question {
        title: "Clear your pack?".into(),
        message: format!(
            "Unpick {}? The game keeps the pack you built until you press b.",
            format::count(b.cart.len(), "tweak")
        ),
        yes: "Clear",
        danger: Some("Your picks can't be brought back.".into()),
        on_yes: Command::ClearNow.action(),
    })))
}

fn ask_remove(b: &Browser, inst: &Inst) -> Handled {
    if !b.is_built() {
        return Handled::Ignored;
    }
    if let Some(wait) = busy(b) {
        return wait;
    }
    Handled::Action(Action::Confirm(Box::new(Question {
        title: "Remove your pack from the game?".into(),
        message: format!(
            "Takes Vanilla Tweaks out of “{}”. Your picks stay here, ready to build again.",
            inst.name
        ),
        yes: "Remove",
        danger: None,
        on_yes: Command::RemoveNow.action(),
    })))
}

fn remove(b: &mut Browser, inst: &Inst, cx: &Ctx<'_>) -> Handled {
    if b.job.is_some() {
        return Handled::Ignored;
    }
    let game = inst.game_dir.clone();
    let mut cart = b.cart.clone();
    let work = async move {
        let removed = vanilla_tweaks::unapply(&game, &mut cart).await;
        removed.map(|()| cart)
    };
    b.jobs += 1;
    let seq = b.jobs;
    let task = cx
        .tasks
        .run(work, move |removed| app_msg(Msg::Removed(seq, removed)));
    b.job = Some(Job::new(seq, Work::Removing, cx.now, task));
    Handled::Consumed
}

/// Fetches the catalog again after an offline start.
fn retry(b: &mut Browser, cx: &Ctx<'_>) -> Handled {
    if !b.offline || b.retry.is_some() {
        return Handled::Ignored;
    }
    let site = cx.services.vanilla_tweaks.clone();
    let version = b.catalog.version;
    let work = async move { site.catalog(version).await };
    b.retry = Some((
        cx.now,
        cx.tasks
            .run(work, |catalog| app_msg(Msg::Refreshed(catalog))),
    ));
    Handled::Consumed
}

/// The right-click menu for the selected pack.
fn menu(b: &Browser, at: Position) -> Handled {
    let Some(row) = b.focused_pack() else {
        return Handled::Consumed;
    };
    let item = |label, key, command: Command| MenuItem {
        label,
        key: KeyCode::Char(key),
        danger: false,
        action: command.action(),
    };
    let toggle = match row.state {
        PackState::Unpicked => "Add to your pack",
        PackState::PendingRemoval => "Keep in your pack",
        PackState::Built | PackState::PendingAdd => "Remove from your pack",
    };
    let mut items = vec![
        item(toggle, ' ', Command::Toggle),
        item("Open preview", 'o', Command::Preview),
    ];
    if b.pack(&row.id)
        .is_some_and(|(_, pack)| pack.video.is_some())
    {
        items.push(item("Watch video", 'w', Command::Video));
    }
    Handled::Action(Action::Menu {
        title: row.label.clone(),
        items,
        anchor: Some(at),
    })
}

/// What space does to a pack in this state.
fn toggle_word(state: PackState) -> &'static str {
    match state {
        PackState::Unpicked => "add",
        PackState::PendingRemoval => "keep",
        PackState::Built | PackState::PendingAdd => "remove",
    }
}

/// "A", "A and B", "A, B and C".
fn and_list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What went wrong talking to the site, and what to do: press `key` to try
/// again.
fn describe(e: &vanilla_tweaks::Error, key: char) -> String {
    match e {
        vanilla_tweaks::Error::Net(net::Error::Request { .. }) => {
            format!("Couldn't reach Vanilla Tweaks — check your connection, then press {key}.")
        }
        vanilla_tweaks::Error::Net(net::Error::Status { status, .. }) => {
            format!("Vanilla Tweaks answered {status} — press {key} to try again.")
        }
        vanilla_tweaks::Error::Net(net::Error::Decode { .. }) => {
            "Vanilla Tweaks sent something mctui couldn't read — try again later.".to_owned()
        }
        e => {
            let e = e.to_string();
            let mut chars = e.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

impl Screen for ResourcePacks {
    fn crumbs(&self, _data: &Data) -> Vec<String> {
        vec![self.inst.name.clone(), "Resource packs".into()]
    }

    fn hints(&self, _data: &Data) -> Vec<Hint> {
        let key = Hint::key;
        let back = key(KeyCode::Esc, "back");
        let help = key(KeyCode::Char('?'), "help");
        let b = match &self.state {
            State::Unsupported(_) => return vec![back],
            State::Loading { .. } => return vec![back, help],
            State::Failed(_) => return vec![key(KeyCode::Char('r'), "retry"), back, help],
            State::Ready(b) => b,
        };
        if b.search.as_ref().is_some_and(|s| s.typing) {
            return vec![
                Hint::show("↑↓", "results"),
                key(KeyCode::Enter, "done"),
                key(KeyCode::Esc, "cancel"),
            ];
        }
        let mut hints = Vec::new();
        match (b.focus, b.focused_pack()) {
            (Pane::Categories, _) => {
                let open = match b.selected_node() {
                    Some(node) if node.expandable && node.expanded => "close",
                    Some(node) if node.expandable => "open",
                    _ => "packs",
                };
                hints.push(key(KeyCode::Enter, open));
            }
            (_, Some(row)) => {
                hints.push(key(KeyCode::Char(' '), toggle_word(row.state)));
                if b.focus == Pane::Cart {
                    hints.push(key(KeyCode::Enter, "show"));
                }
            }
            (_, None) => {}
        }
        if b.search.is_none() && b.focus != Pane::Cart {
            hints.push(key(KeyCode::Char('/'), "search"));
        }
        if b.job.is_none() && b.needs_build() {
            hints.push(key(KeyCode::Char('b'), "build & apply"));
        }
        if b.focus == Pane::Cart && !b.cart.is_empty() {
            hints.push(key(KeyCode::Char('c'), "clear"));
        }
        if b.focus == Pane::Cart && b.is_built() {
            hints.push(key(KeyCode::Char('u'), "remove from game"));
        }
        if b.offline && b.retry.is_none() {
            hints.push(key(KeyCode::Char('r'), "retry"));
        }
        hints.push(key(
            KeyCode::Esc,
            if b.search.is_some() {
                "end search"
            } else if b.job.is_some() {
                "stop"
            } else {
                "back"
            },
        ));
        // Help is a keypress away anyway; it goes first when room is short.
        hints.push(help);
        if ui::hints_height(&hints, self.body.width) > 1 {
            hints.pop();
        }
        hints
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        let groups = vec![
            HelpGroup {
                title: "Move",
                entries: &[
                    ("↑↓ j k", "move"),
                    ("tab ←→", "next · previous pane"),
                    ("→ ←", "open · close a category"),
                    ("/", "search every pack"),
                ],
            },
            HelpGroup {
                title: "Your pack",
                entries: &[
                    ("space ↵", "add · remove a pack"),
                    ("b", "build & apply"),
                    ("c", "clear your pack"),
                    ("u", "remove from the game"),
                ],
            },
            HelpGroup {
                title: "Packs",
                entries: &[("o", "open its preview"), ("w", "watch its video")],
            },
            HelpGroup {
                title: "General",
                entries: &[
                    ("r", "retry"),
                    ("esc", "end search · stop · back"),
                    ("?", "help"),
                    ("double-click", "add · remove"),
                    ("right-click", "pack actions"),
                ],
            },
        ];
        ("Resource packs", groups)
    }

    fn palette(&self, _data: &Data) -> Vec<PaletteItem> {
        let State::Ready(b) = &self.state else {
            return Vec::new();
        };
        let verb = |label: &str, key, command: Command| PaletteItem {
            label: label.to_owned(),
            key: Some(key),
            jump: false,
            action: command.action(),
        };
        let mut items = vec![verb("Search packs", "/", Command::Search)];
        if b.needs_build() {
            items.push(verb("Build & apply your pack", "b", Command::Build));
        }
        if !b.cart.is_empty() {
            items.push(verb("Clear your pack", "c", Command::Clear));
        }
        if b.is_built() {
            items.push(verb("Remove your pack from the game", "u", Command::Remove));
        }
        items
    }

    fn captures_text(&self) -> bool {
        matches!(&self.state, State::Ready(b) if b.search.as_ref().is_some_and(|s| s.typing))
    }

    fn set_body(&mut self, body: Rect) {
        self.body = body;
        self.fit();
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        self.on_key(key, cx)
    }

    fn handle_mouse(&mut self, target: &Target, mouse: Mouse, cx: &mut Ctx<'_>) -> Handled {
        self.on_mouse(target, mouse, cx)
    }

    fn handle_paste(&mut self, text: &str, _cx: &mut Ctx<'_>) -> Handled {
        let State::Ready(b) = &mut self.state else {
            return Handled::Ignored;
        };
        match &mut b.search {
            Some(search) if search.typing => {
                if search.input.insert(text) {
                    b.searched();
                }
                Handled::Consumed
            }
            _ => Handled::Ignored,
        }
    }

    fn command(&mut self, command: screen::Command, cx: &mut Ctx<'_>) -> Handled {
        match command {
            screen::Command::ResourcePacks(command) => self.run(command, cx),
            screen::Command::Mods(_) => Handled::Ignored,
        }
    }

    fn handle_msg(&mut self, msg: crate::msg::Msg, cx: &mut Ctx<'_>) -> Handled {
        let crate::msg::Msg::ResourcePacks(msg) = msg else {
            return Handled::Ignored;
        };
        self.on_msg(msg, cx)
    }

    fn reload(&mut self, data: &Data) {
        let renamed = data
            .instances
            .iter()
            .find(|i| i.game_dir() == self.inst.game_dir);
        if let Some(inst) = renamed {
            self.inst.name.clone_from(&inst.name);
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.spinning_since()
            .map(|since| widgets::next_spin(since, self.frame))
    }

    fn tick(&mut self, now: Instant) -> bool {
        let spinning = self.spinning_since().is_some();
        if spinning {
            self.frame = now;
        }
        spinning
    }

    fn render(&self, cv: &mut Canvas<'_>, _data: &Data) {
        self.draw(cv);
    }
}
