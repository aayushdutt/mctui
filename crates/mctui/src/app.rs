//! The app: routes input to the overlay or screen that owns it, runs the
//! actions they ask for, and lays out the shared chrome (header, banners,
//! footer) around the screen.

use std::time::{Duration, Instant};

use crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use mctui_core::facets;
use mctui_core::instance::Instance;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;

use crate::accounts::{self, AccountList, Checks, OfflineProfile, SignIn};
use crate::data::Data;
use crate::edit::EditInstance;
use crate::format;
use crate::home::Home;
use crate::mods::{self, Mods};
use crate::msg::Msg;
use crate::overlay::{Confirm, Help, Menu, MenuItem, Outcome, Overlay, Palette, PaletteItem};
use crate::play::{self, Phase, PlayScreen, Session};
use crate::resource_packs::ResourcePacks;
use crate::screen::{Ctx, Handled, Mouse, Screen, ScreenId};
use crate::services::Services;
use crate::settings::Settings;
use crate::task::{Event, Tasks};
use crate::theme::{Terminal, Theme};
use crate::ui::{self, Canvas, Cx, Hits, Target, text};
use crate::wizard::Wizard;

/// How long a banner stays up.
const BANNER_TTL: Duration = Duration::from_secs(4);
/// Two clicks on the same spot within this are a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(350);

/// A screen to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    NewInstance,
    Settings,
    Accounts,
    // Microsoft sign-in, then `then`: the launch that needed it.
    SignIn { then: Option<Box<Self>> },
    OfflineProfile,
    Mods(String),
    ResourcePacks(String),
    EditInstance(String),
    Launch { id: String, offline: bool },
}

/// Something only the app can do, asked for by a screen, overlay or click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    Banner(Banner),
    Open(Route),
    /// Close the top screen.
    Close,
    /// Switch themes (settings previews them live).
    SetTheme(String),
    /// Instances changed on disk: rescan and refresh every screen.
    Reload,
    /// Ask first; run `on_yes` if confirmed.
    Confirm(Box<Question>),
    Many(Vec<Action>),
    OpenFolder(String),
    /// Ask before deleting.
    ConfirmDelete(String),
    Delete(String),
    /// Jump to an instance on home.
    Select(String),
    ActionsMenu {
        id: String,
        anchor: Option<Position>,
    },
    /// Run the active screen's command `name` (see [`Screen::command`]).
    Command(&'static str),
    /// A small menu of actions, at the pointer when `anchor` is given.
    Menu {
        title: String,
        items: Vec<MenuItem>,
        anchor: Option<Position>,
    },
    /// Change the accounts; the app saves them and checks the session.
    Account(accounts::Change),
    /// Put text on the clipboard.
    Copy(String),
    OpenUrl(String),
    /// Hand a choice from a menu, dialog or the palette back to the mods screen.
    Mods(mods::Command),
    /// Ask the running game to save and quit.
    StopGame,
    /// Back out of every screen the way esc would, stopping at any question
    /// (unsaved edits) a screen asks on the way.
    Home,
}

impl Action {
    pub fn launch(inst: &Instance, offline: bool) -> Self {
        Self::Open(Route::Launch {
            id: inst.id.clone(),
            offline,
        })
    }
}

/// A yes/no question for [`Action::Confirm`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub title: String,
    pub message: String,
    pub yes: &'static str,
    /// A caution line; makes the dialog destructive (red).
    pub danger: Option<String>,
    pub on_yes: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    /// Guidance ("Press q to quit.").
    Info,
    /// It worked.
    Success,
    /// It didn't.
    Warn,
}

/// A one-line message above the footer that clears itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub kind: BannerKind,
    pub text: String,
}

impl Banner {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            kind: BannerKind::Info,
            text: text.into(),
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            kind: BannerKind::Success,
            text: text.into(),
        }
    }

    pub fn warn(text: impl Into<String>) -> Self {
        Self {
            kind: BannerKind::Warn,
            text: text.into(),
        }
    }
}

/// Where the shared chrome and the screen go.
#[derive(Debug, Clone, Copy, Default)]
struct Chrome {
    header: Rect,
    body: Rect,
    notices: Rect,
    banner: Rect,
    footer: Rect,
}

/// Home's id: it is always at the bottom of the stack.
const HOME: ScreenId = 0;
/// The app's own background work (session checks) reports to this id.
const APP: ScreenId = ScreenId::MAX;

#[derive(Debug)]
pub struct App {
    data: Data,
    terminal: Terminal,
    theme: Theme,
    home: Home,
    /// Screens opened over home, top last.
    stack: Vec<(ScreenId, Box<dyn Screen>)>,
    next_id: ScreenId,
    tasks: Tasks,
    services: Services,
    overlay: Option<Overlay>,
    banner: Option<(Banner, Instant)>,
    /// Problems found at startup; they stay until the next key.
    notices: Vec<String>,
    size: Rect,
    /// Last known pointer position, and what's under it.
    pointer: Option<Position>,
    hover: Option<Target>,
    /// The last frame's clickable regions.
    hits: Hits,
    /// The last left click, for spotting double-clicks on the same thing.
    last_click: Option<(Instant, Target)>,
    /// Where the chrome and screen go, and what the footer offers; recomputed
    /// whenever state changes, reused by every frame until then.
    chrome: Chrome,
    footer: Vec<ui::Hint>,
    notice_lines: Vec<String>,
    quit: bool,
    /// A quit was asked for while the game runs; the next `q` goes through.
    quit_armed: bool,
    /// The last launch session's number.
    sessions: u64,
    /// Session checks the app runs for the accounts.
    checks: Checks,
    /// Text to put on the clipboard after the next frame.
    clipboard: Option<String>,
    /// Opens a folder or a web page (swapped out in tests).
    open: fn(&std::ffi::OsStr) -> std::io::Result<()>,
}

impl App {
    pub fn new(
        data: Data,
        notices: Vec<String>,
        terminal: Terminal,
        (tasks, services): (Tasks, Services),
        size: Rect,
    ) -> Self {
        let home = Home::new(&data, splash());
        let theme = Theme::resolve(&data.config.theme, terminal);
        let mut app = Self {
            data,
            terminal,
            theme,
            home,
            stack: Vec::new(),
            next_id: HOME + 1,
            tasks,
            services,
            overlay: None,
            banner: None,
            notices,
            size,
            pointer: None,
            hover: None,
            hits: Hits::default(),
            last_click: None,
            chrome: Chrome::default(),
            footer: Vec::new(),
            notice_lines: Vec::new(),
            quit: false,
            quit_armed: false,
            sessions: 0,
            checks: Checks::default(),
            clipboard: None,
            open: |target| open::that_detached(target),
        };
        app.relayout();
        app
    }

    /// Starts the background work a fresh run begins with: checking the
    /// active account's session.
    pub fn start(&mut self) {
        let tasks = self.tasks.for_screen(APP);
        self.checks
            .check_active(&mut self.data, &tasks, &self.services);
    }

    /// Text the app asked to copy, for the terminal to write after a frame.
    pub fn take_clipboard(&mut self) -> Option<String> {
        self.clipboard.take()
    }

    #[cfg(test)]
    pub fn data_mut(&mut self) -> &mut Data {
        &mut self.data
    }

    /// Opens the launch screen for `id` without starting a launch.
    #[cfg(test)]
    pub fn show_play_screen_for_tests(&mut self, id: &str) {
        self.stack
            .push((self.next_id, Box::new(PlayScreen::new(id))));
        self.next_id += 1;
        self.relayout();
    }

    /// Pretends `n` launches have happened, so a test's session can be the latest.
    #[cfg(test)]
    pub fn sessions_for_tests(&mut self, n: u64) {
        self.sessions = n;
    }

    #[cfg(test)]
    pub fn set_splash(&mut self, splash: &'static str) {
        self.home.set_splash(splash);
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// The render context for a frame.
    pub fn cx(&self, now: jiff::Timestamp, tz: jiff::tz::TimeZone) -> Cx {
        Cx {
            theme: self.theme,
            hover: self.hover.clone(),
            now,
            tz,
        }
    }

    fn active(&self) -> &dyn Screen {
        self.stack
            .last()
            .map_or(&self.home as &dyn Screen, |(_, s)| s.as_ref())
    }

    fn active_id(&self) -> ScreenId {
        self.stack.last().map_or(HOME, |(id, _)| *id)
    }

    /// Runs `f` on screen `id` (if it's still open) with a context for it.
    fn with_screen<R>(
        &mut self,
        id: ScreenId,
        f: impl FnOnce(&mut dyn Screen, &mut Ctx<'_>) -> R,
    ) -> Option<R> {
        let screen: &mut dyn Screen = if id == HOME {
            &mut self.home
        } else {
            self.stack
                .iter_mut()
                .find(|(sid, _)| *sid == id)?
                .1
                .as_mut()
        };
        let tasks = self.tasks.for_screen(id);
        let mut cx = Ctx {
            data: &mut self.data,
            services: &self.services,
            tasks: &tasks,
            theme: &self.theme,
            now: Instant::now(),
        };
        Some(f(screen, &mut cx))
    }

    /// Runs `f` on the active screen and acts on what it says.
    fn on_active(&mut self, f: impl FnOnce(&mut dyn Screen, &mut Ctx<'_>) -> Handled) -> bool {
        let handled = self
            .with_screen(self.active_id(), f)
            .unwrap_or(Handled::Ignored);
        self.apply(handled)
    }

    /// Acts on a screen's answer. Returns whether to redraw.
    fn apply(&mut self, handled: Handled) -> bool {
        match handled {
            Handled::Ignored => false,
            Handled::Consumed => true,
            Handled::Action(action) => {
                self.run(action);
                true
            }
        }
    }

    /// When the app next needs to wake without input.
    pub fn next_deadline(&self) -> Option<Instant> {
        let banner = self.banner.as_ref().map(|(_, until)| *until);
        let screen = self.active().next_deadline();
        // The launch screen's clock lives with its session.
        let game = self
            .data
            .game
            .as_ref()
            .filter(|g| self.active().plays(&g.instance_id))
            .and_then(|g| g.next_deadline(Instant::now()));
        banner.into_iter().chain(screen).chain(game).min()
    }

    /// Time passed. Returns whether to redraw.
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut redraw = false;
        if self.banner.as_ref().is_some_and(|(_, until)| *until <= now) {
            self.banner = None;
            self.relayout();
            redraw = true;
        }
        let id = self.active_id();
        redraw |= self.with_screen(id, |s, _| s.tick(now)).unwrap_or(false);
        // The launch screen's spinner and clock move with its session.
        redraw |=
            self.data.game.as_ref().is_some_and(|g| {
                self.active().plays(&g.instance_id) && g.next_deadline(now).is_some()
            });
        redraw
    }

    /// Handles one event. Returns whether to redraw.
    pub fn handle(&mut self, event: Event) -> bool {
        let redraw = match event {
            Event::Msg(APP, msg) => self.on_app_msg(msg),
            Event::Msg(id, msg) => {
                let handled = self.with_screen(id, |s, cx| s.handle_msg(msg, cx));
                handled.is_some_and(|h| self.apply(h))
            }
            Event::Term(TermEvent::Key(key)) if key.kind != KeyEventKind::Release => {
                self.on_key(key)
            }
            Event::Term(TermEvent::Mouse(mouse)) => self.on_mouse(mouse),
            Event::Term(TermEvent::Paste(text)) => self.on_paste(&text),
            Event::Term(TermEvent::Resize(w, h)) => {
                self.size = Rect::new(0, 0, w, h);
                true
            }
            Event::Term(TermEvent::FocusGained) => {
                // Pick up instances added or edited while we were away.
                self.reload();
                true
            }
            Event::Term(TermEvent::Key(_) | TermEvent::FocusLost) => false,
        };
        if redraw {
            self.relayout();
        }
        redraw
    }

    /// Background work the app owns reported back.
    fn on_app_msg(&mut self, msg: Msg) -> bool {
        match msg {
            Msg::Accounts(msg) => {
                if let Some(banner) = self.checks.handle(msg, &mut self.data) {
                    self.show(banner);
                }
                true
            }
            // Screens' own work reports to them, never to the app.
            Msg::Play(seq, msg) => self.on_play_msg(seq, msg),
            Msg::Wizard(_) | Msg::Mods(_) | Msg::ResourcePacks(_) => false,
            #[cfg(test)]
            Msg::Test(_) => false,
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.request_quit();
            return true;
        }
        // A quit asked for while the game runs waits for a second press;
        // any other key disarms it.
        let armed = std::mem::take(&mut self.quit_armed);
        if armed && key.code == KeyCode::Char('q') {
            self.quit = true;
            return true;
        }
        if let Some(overlay) = &mut self.overlay {
            let outcome = overlay.handle_key(key);
            self.finish_overlay(outcome);
            return true;
        }
        // Any key dismisses startup notices, and still does its job.
        let dismissed = !self.notices.is_empty();
        self.notices.clear();

        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if plain && !self.active().captures_text() {
            match key.code {
                KeyCode::Char('?') => {
                    let (title, groups) = self.active().help();
                    self.overlay = Some(Overlay::Help(Help::new(title, groups)));
                    return true;
                }
                KeyCode::Char(':') => {
                    self.overlay = Some(Overlay::Palette(Palette::new(self.palette_items())));
                    return true;
                }
                _ => {}
            }
        }
        if self.on_active(|s, cx| s.handle_key(key, cx)) {
            return true;
        }
        // Unhandled esc on a screen over home goes back.
        if key.code == KeyCode::Esc && !self.stack.is_empty() {
            self.run(Action::Close);
            return true;
        }
        dismissed
    }

    /// The `:` palette: the screen's commands, then the app's.
    fn palette_items(&self) -> Vec<PaletteItem> {
        let mut items = self.active().palette(&self.data);
        let verb = |label: &str, key, action| PaletteItem {
            label: label.to_owned(),
            key,
            jump: false,
            action,
        };
        if !self.stack.is_empty() {
            items.push(verb("Home", Some("esc"), Action::Home));
        }
        if self.active().route() != Some(Route::Accounts) {
            items.push(verb("Accounts", Some("a"), Action::Open(Route::Accounts)));
        }
        items.push(verb("Quit mctui", None, Action::Quit));
        items
    }

    fn on_paste(&mut self, text: &str) -> bool {
        match &mut self.overlay {
            Some(overlay) => {
                overlay.handle_paste(text);
                true
            }
            None => self.on_active(|s, cx| s.handle_paste(text, cx)),
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent) -> bool {
        let pos = Position::new(mouse.column, mouse.row);
        self.pointer = Some(pos);
        let target = self.hits.at(pos).cloned();
        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                let mut changed = false;
                if let (Some(overlay), Some(Target::Overlay(t))) = (&mut self.overlay, &target) {
                    changed |= overlay.hover(t);
                }
                if self.hover != target {
                    self.hover = target;
                    changed = true;
                }
                changed
            }
            MouseEventKind::Down(button) => {
                let Some(target) = target else {
                    self.last_click = None;
                    return false;
                };
                let left = button == MouseButton::Left;
                let double = left
                    && self
                        .last_click
                        .as_ref()
                        .is_some_and(|(at, t)| at.elapsed() <= DOUBLE_CLICK && *t == target);
                // A double-click uses up both clicks, so a third starts over.
                self.last_click = (left && !double).then(|| (Instant::now(), target.clone()));
                self.on_click(target, button, pos, double)
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let down = mouse.kind == MouseEventKind::ScrollDown;
                match (&mut self.overlay, target) {
                    (Some(overlay), _) => overlay.scroll(down),
                    (None, Some(target)) => {
                        self.on_active(|s, cx| s.handle_mouse(&target, Mouse::Scroll { down }, cx))
                    }
                    (None, None) => false,
                }
            }
            _ => false,
        }
    }

    fn on_click(
        &mut self,
        target: Target,
        button: MouseButton,
        pos: Position,
        double: bool,
    ) -> bool {
        let left = button == MouseButton::Left;
        if let Some(overlay) = &mut self.overlay {
            // The overlay used this click; it can't start a double-click.
            self.last_click = None;
            let outcome = match target {
                Target::Backdrop => Outcome::Close,
                Target::Overlay(t) if left => overlay.click(&t),
                Target::Key(code) if left => return self.on_key(KeyEvent::from(code)),
                _ => Outcome::Stay,
            };
            self.finish_overlay(outcome);
            return true;
        }
        let dismissed = !self.notices.is_empty();
        self.notices.clear();
        // Only list rows mean something on a double-click; a button would
        // just act twice.
        if double && !target.is_row() {
            return dismissed;
        }
        let acted = match target {
            Target::Account if left => {
                if self.stack.is_empty() {
                    self.home.commit_filter(&self.data);
                }
                self.run(Action::Open(Route::Accounts));
                true
            }
            Target::Key(code) if left => {
                // A clicked letter is a command, not something to type.
                if matches!(code, KeyCode::Char(_)) && self.stack.is_empty() {
                    self.home.commit_filter(&self.data);
                }
                self.on_key(KeyEvent::from(code))
            }
            target if left => {
                self.on_active(|s, cx| s.handle_mouse(&target, Mouse::Click { double }, cx))
            }
            target if button == MouseButton::Right => {
                self.on_active(|s, cx| s.handle_mouse(&target, Mouse::RightClick(pos), cx))
            }
            _ => false,
        };
        acted || dismissed
    }

    fn finish_overlay(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Stay => {}
            Outcome::Close => self.overlay = None,
            Outcome::Run(action) => {
                self.overlay = None;
                self.run(action);
            }
        }
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::Quit => self.request_quit(),
            Action::Home => {
                // Each esc lets a screen revert a preview, cancel its work,
                // or ask about unsaved edits (which stops the walk).
                for _ in 0..self.stack.len() * 3 {
                    if self.stack.is_empty() || self.overlay.is_some() {
                        break;
                    }
                    self.on_key(KeyEvent::from(KeyCode::Esc));
                }
            }
            Action::StopGame => {
                if let Some(game) = &mut self.data.game {
                    game.stop();
                }
            }
            Action::Banner(banner) => self.show(banner),
            Action::Open(route) => self.open(&route),
            Action::Close => {
                self.stack.pop();
            }
            Action::SetTheme(name) => self.theme = Theme::resolve(&name, self.terminal),
            Action::Reload => self.reload(),
            Action::Confirm(q) => {
                let dialog = Confirm::new(q.title, q.message, q.yes, q.on_yes);
                let dialog = match q.danger {
                    Some(warning) => dialog.danger(warning),
                    None => dialog,
                };
                self.overlay = Some(Overlay::Confirm(dialog));
            }
            Action::Many(actions) => actions.into_iter().for_each(|a| self.run(a)),
            Action::OpenFolder(id) => self.open_folder(&id),
            Action::ConfirmDelete(id) => self.confirm_delete(&id),
            Action::Delete(id) => self.delete(&id),
            Action::Select(id) => self.home.select_id(&id, &self.data),
            Action::ActionsMenu { id, anchor } => {
                if let Some(inst) = self.data.instance(&id) {
                    let menu = Menu::new(inst.name.clone(), Home::menu_items(inst), anchor);
                    self.overlay = Some(Overlay::Menu(menu));
                }
            }
            Action::Command(name) => {
                self.on_active(|s, cx| s.command(name, cx));
            }
            Action::Menu {
                title,
                items,
                anchor,
            } => self.overlay = Some(Overlay::Menu(Menu::new(title, items, anchor))),
            Action::Account(change) => {
                let tasks = self.tasks.for_screen(APP);
                if let Some(banner) =
                    self.checks
                        .apply(change, &mut self.data, &tasks, &self.services)
                {
                    self.show(banner);
                }
                for (_, screen) in &mut self.stack {
                    screen.reload(&self.data);
                }
            }
            Action::Copy(text) => self.clipboard = Some(text),
            Action::OpenUrl(url) => {
                if let Err(e) = (self.open)(url.as_ref()) {
                    self.show(Banner::warn(format!(
                        "Couldn't open your browser ({e}) — go to {url} yourself."
                    )));
                }
            }
            Action::Mods(command) => {
                let msg = Msg::Mods(mods::Msg::Chose(command));
                if let Some(handled) =
                    self.with_screen(self.active_id(), |s, cx| s.handle_msg(msg, cx))
                {
                    self.apply(handled);
                }
            }
        }
    }

    /// Opens a screen over the current one.
    fn open(&mut self, route: &Route) {
        // Never stack a second copy of the screen on top.
        if self
            .stack
            .last()
            .is_some_and(|(_, s)| s.route().as_ref() == Some(route))
        {
            return;
        }
        // The running game's instance can't change under it.
        if let Route::EditInstance(id) | Route::Mods(id) | Route::ResourcePacks(id) = route
            && play::is_playing(&self.data, id)
        {
            return self.show(Banner::warn(
                "Close Minecraft before changing this instance.",
            ));
        }
        let screen: Box<dyn Screen> = match route {
            Route::Settings => Box::new(Settings::new(&self.data)),
            Route::EditInstance(id) => match self.data.instance(id) {
                Some(inst) => Box::new(EditInstance::new(inst)),
                None => return,
            },
            Route::Accounts => Box::new(AccountList::new(&self.data)),
            Route::SignIn { then } => {
                let tasks = self.tasks.for_screen(self.next_id);
                let then = then.as_deref().cloned();
                Box::new(SignIn::new(then, &self.data, &self.services, &tasks))
            }
            Route::OfflineProfile => Box::new(OfflineProfile::new()),
            Route::NewInstance => {
                // Its background work reports to the id it's about to get.
                let tasks = self.tasks.for_screen(self.next_id);
                Box::new(Wizard::new(&self.data, &self.services, &tasks))
            }
            // A game already running only needs showing (or refusing).
            Route::Launch { id, offline }
                if self.data.game.as_ref().is_some_and(Session::is_live) =>
            {
                return self.launch(id, *offline);
            }
            // A-8: playing online needs a sign-in that works; get one first,
            // then carry on with the launch.
            Route::Launch { offline: false, .. } if accounts::needs_sign_in(&self.data) => {
                let then = Some(Box::new(route.clone()));
                return self.open(&Route::SignIn { then });
            }
            Route::Launch { id, offline } => return self.launch(id, *offline),
            Route::Mods(id) => match self.data.instance(id) {
                Some(inst) => Box::new(Mods::new(
                    inst,
                    self.tasks.for_screen(self.next_id),
                    self.services.modrinth.clone(),
                )),
                None => return,
            },
            Route::ResourcePacks(id) => match self.data.instance(id) {
                Some(inst) => {
                    let tasks = self.tasks.for_screen(self.next_id);
                    Box::new(ResourcePacks::new(inst, &self.services, &tasks))
                }
                None => return,
            },
        };
        let id = self.next_id;
        self.next_id += 1;
        self.stack.push((id, screen));
        self.overlay = None;
    }

    /// Plays `id`: shows the running session if it's this instance's, says
    /// no if another game is running, else starts a launch.
    fn launch(&mut self, id: &str, offline: bool) {
        if let Some(game) = self.data.game.as_ref().filter(|g| g.is_live()) {
            if game.instance_id != id {
                let name = game.instance_name.clone();
                return self.show(Banner::warn(format!(
                    "{name} is still running. Stop it before playing another instance."
                )));
            }
        } else {
            self.sessions += 1;
            let tasks = self.tasks.for_screen(APP);
            let session = Session::start(
                self.sessions,
                id,
                offline,
                &self.data,
                &self.services,
                &tasks,
            );
            let Some(session) = session else {
                return;
            };
            self.data.game = Some(session);
            self.home.reload(&self.data);
        }
        // Reuse the launch screen if it's already on top.
        if matches!(self.stack.last(), Some((_, s)) if s.plays(id)) {
            return;
        }
        let screen_id = self.next_id;
        self.next_id += 1;
        self.stack.push((screen_id, Box::new(PlayScreen::new(id))));
        self.overlay = None;
    }

    /// Applies a launch's message, if it's from the current session.
    fn on_play_msg(&mut self, seq: u64, msg: play::Msg) -> bool {
        let current = self.data.game.as_ref().is_some_and(|g| g.seq == seq);
        if !current {
            // A launch cancelled just as its game started: nothing tracks
            // that game, so stop it rather than leave it running.
            if let play::Msg::Started(started) = msg {
                started.game.stop();
            }
            return false;
        }
        let Some(mut game) = self.data.game.take() else {
            return false;
        };
        // Output only needs a redraw if the launch screen shows a new line.
        let showing = matches!(self.stack.last(), Some((_, s)) if s.plays(&game.instance_id));
        let redraw = match &msg {
            play::Msg::Logs(lines) => {
                let level = self.data.config.launch_log_verbosity;
                showing && lines.iter().any(|l| l.level.shown_at(level))
            }
            _ => true,
        };
        let exited = matches!(msg, play::Msg::Exited(_));
        let started = matches!(msg, play::Msg::Started(_));
        let banner = game.apply(msg, &mut self.data);
        let clean_exit = exited && matches!(&game.phase, Phase::Ended(r) if !r.crashed());
        let id = game.instance_id.clone();
        self.data.game = Some(game);
        if let Some(banner) = banner {
            self.show(banner);
        }
        if exited || started {
            self.home.reload(&self.data);
        }
        // A clean exit goes back to where you were; a crash stays to show why.
        if clean_exit && matches!(self.stack.last(), Some((_, s)) if s.plays(&id)) {
            self.stack.pop();
        }
        redraw
    }

    /// Quits, unless the game is running: then the first request warns and
    /// the second goes through.
    fn request_quit(&mut self) {
        let running = self.data.game.as_ref().is_some_and(Session::is_running);
        if running && !self.quit_armed {
            self.quit_armed = true;
            self.show(Banner::warn(
                "Minecraft is still running. Press q again to quit mctui: the game keeps running, but this session's play time won't be recorded.",
            ));
        } else {
            self.quit = true;
        }
    }

    fn show(&mut self, banner: Banner) {
        self.banner = Some((banner, Instant::now() + BANNER_TTL));
    }

    fn reload(&mut self) {
        if let Some(problem) = self.data.reload_instances() {
            self.show(Banner::warn(problem));
        }
        self.home.reload(&self.data);
        for (_, screen) in &mut self.stack {
            screen.reload(&self.data);
        }
    }

    fn open_folder(&mut self, id: &str) {
        let Some(inst) = self.data.instance(id) else {
            return;
        };
        let dir = inst.game_dir();
        let opened = std::fs::create_dir_all(&dir).and_then(|()| (self.open)(dir.as_os_str()));
        let banner = match opened {
            Ok(()) => Banner::info(format!("Opened “{}” in your file manager.", inst.name)),
            Err(e) => Banner::warn(format!("Couldn't open {}: {e}", dir.display())),
        };
        self.show(banner);
    }

    fn confirm_delete(&mut self, id: &str) {
        if play::is_playing(&self.data, id) {
            return self.show(Banner::warn(
                "Close Minecraft before deleting this instance.",
            ));
        }
        let Some(inst) = self.data.instance(id) else {
            return;
        };
        let mut lost = Vec::new();
        let worlds = facets::worlds(inst).len();
        if worlds > 0 {
            lost.push(format::count(worlds, "world"));
        }
        let mods = facets::mod_jars(inst).len();
        if mods > 0 {
            lost.push(format::count(mods, "mod"));
        }
        if inst.play_time_secs > 0 {
            lost.push(format!("{} played", format::play_time(inst.play_time_secs)));
        }
        let what = if lost.is_empty() {
            "Removes its folder.".to_owned()
        } else {
            format!("Removes its folder: {}.", lost.join(" · "))
        };
        self.run(Action::Confirm(Box::new(Question {
            title: "Delete instance?".into(),
            message: format!("Delete “{}”?\n{what}", inst.name),
            yes: "Delete",
            danger: Some("This cannot be undone.".into()),
            on_yes: Action::Delete(id.to_owned()),
        })));
    }

    fn delete(&mut self, id: &str) {
        if play::is_playing(&self.data, id) {
            return self.show(Banner::warn(
                "Close Minecraft before deleting this instance.",
            ));
        }
        let Some(inst) = self.data.instance(id) else {
            return;
        };
        let name = inst.name.clone();
        match self.data.store.delete(inst) {
            Ok(()) => {
                self.data.instances.retain(|i| i.id != id);
                self.home.reload(&self.data);
                self.show(Banner::success(format!("Deleted “{name}”.")));
            }
            Err(e) => self.show(Banner::warn(format!("Couldn't delete “{name}”: {e}"))),
        }
    }

    /// Recomputes the chrome and footer after a state change, and tells the
    /// active screen its new body.
    fn relayout(&mut self) {
        // A little air around everything: 1 row top and bottom, 2 columns each side.
        let area = self.size;
        let inner = if area.width >= 12 && area.height >= 6 {
            Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2)
        } else {
            area
        };
        self.footer = self.active().hints(&self.data);
        self.notice_lines = notice_lines(&self.notices, inner.width);
        // The footer never crowds out the header: on a tiny terminal it gets
        // what's left, or nothing.
        let footer_h =
            ui::hints_height(&self.footer, inner.width).min(inner.height.saturating_sub(3));
        let notices_h = self.notice_lines.len() as u16;
        let banner_h = u16::from(self.banner.is_some());
        let footer = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(footer_h),
            inner.width,
            footer_h,
        );
        let banner = Rect::new(
            inner.x,
            footer.y.saturating_sub(1 + banner_h),
            inner.width,
            banner_h,
        );
        let notices = Rect::new(
            inner.x,
            banner.y.saturating_sub(notices_h),
            inner.width,
            notices_h,
        );
        let body_top = inner.y + 2;
        self.chrome = Chrome {
            header: Rect {
                height: 1.min(inner.height),
                ..inner
            },
            body: Rect::new(
                inner.x,
                body_top,
                inner.width,
                notices.y.saturating_sub(body_top),
            ),
            notices,
            banner,
            footer,
        };
        let body = self.chrome.body;
        let id = self.active_id();
        self.with_screen(id, |s, _| s.set_body(body));
        // The body may have changed the footer (a pane that no longer fits).
        self.footer = self.active().hints(&self.data);
    }

    /// Draws a frame. Returns its clickable regions and where the cursor goes.
    pub fn render(&self, frame: &mut Frame<'_>, cx: &Cx) -> (Hits, Option<Position>) {
        let area = frame.area();
        let mut cv = Canvas::new(frame.buffer_mut(), cx);
        let chrome = self.chrome;
        let t = self.theme;

        let crumbs = self.active().crumbs(&self.data);
        let crumbs: Vec<&str> = crumbs.iter().map(String::as_str).collect();
        let account = self.account_status();
        cv.header(chrome.header, &crumbs, Some((account, Target::Account)));
        let screen = self.active();
        cv.clipped(chrome.body, |cv| screen.render(cv, &self.data));

        let lines = &self.notice_lines;
        for (i, line) in lines.iter().enumerate() {
            let fg = if i + 1 == lines.len() {
                t.muted
            } else {
                t.warning
            };
            let area = chrome.notices;
            cv.text(
                area.x,
                area.y + i as u16,
                line,
                area.width,
                Style::new().fg(fg),
            );
        }
        if let Some((banner, _)) = &self.banner {
            // A toast: one tinted row, coloured by what it says.
            let (glyph, fg, bg) = match banner.kind {
                BannerKind::Info => (None, t.text, t.surface),
                BannerKind::Success => (Some(ui::CHECK), t.success, t.success_bg),
                BannerKind::Warn => (Some(ui::WARN), t.warning, t.warning_bg),
            };
            let line = match glyph {
                Some(glyph) => format!("{glyph}  {}", banner.text),
                None => banner.text.clone(),
            };
            let area = chrome.banner;
            let line =
                text::truncate(&line, usize::from(area.width.saturating_sub(2))).into_owned();
            cv.pill(
                area.x,
                area.y,
                &Line::styled(line, Style::new().fg(fg)),
                bg,
                area.width,
            );
        }
        cv.hints(chrome.footer, &self.footer);

        if let Some(overlay) = &self.overlay {
            cv.cursor = None;
            overlay.render(&mut cv, area);
        }
        (cv.hits, cv.cursor)
    }

    /// Takes the last frame's hit map. Returns whether the pointer now rests on
    /// something else (the frame moved under it), which needs another redraw.
    pub fn set_hits(&mut self, hits: Hits) -> bool {
        self.hits = hits;
        let hover = self.pointer.and_then(|p| self.hits.at(p).cloned());
        std::mem::replace(&mut self.hover, hover) != self.hover
    }

    /// The header's right side: who's signed in.
    fn account_status(&self) -> Line<'static> {
        accounts::status_line(&self.data, &self.theme)
    }
}

/// Startup notices wrapped to `width`, with a dismiss hint.
fn notice_lines(notices: &[String], width: u16) -> Vec<String> {
    if notices.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<String> = notices
        .iter()
        .flat_map(|n| text::wrap(&format!("{} {n}", ui::WARN), usize::from(width)))
        .collect();
    lines.push("  press any key to dismiss".to_owned());
    lines
}

/// A Minecraft-title-screen wink under the empty home's wordmark, picked once
/// per run.
fn splash() -> &'static str {
    const SPLASHES: &[&str] = &[
        "Now with 100% more terminal!",
        "Also try NetHack!",
        "Ctrl+C safe!",
        "Mouse optional!",
        "Runs on your fridge, probably!",
        "So indie it hurts!",
        "Blazingly monospaced!",
        "As seen in a terminal!",
        "Pixels are optional!",
        "Powered by good intentions!",
        "Fabric-tested, vanilla-approved!",
        "Zero telemetry, all vibes!",
        "Press ? for secrets!",
        "Punch some trees!",
        "Diamonds are down there!",
        "Creeper? Aww man.",
        "Emeralds not included!",
        "May contain traces of Java!",
        "Loads faster than a nether portal!",
        "Keyboard-first, mouse-friendly!",
        "Mine deeper. Type faster.",
        "Rewritten in Rust, obviously!",
    ];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    SPLASHES[nanos as usize % SPLASHES.len()]
}

#[cfg(test)]
pub(crate) mod tests;
