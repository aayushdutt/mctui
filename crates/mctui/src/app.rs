//! The app: routes input to the overlay or screen that owns it, runs the
//! actions they ask for, and lays out the shared chrome (header, banners,
//! footer) around the screen.

use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use mctui_core::account::AccountKind;
use mctui_core::facets;
use mctui_core::instance::Instance;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::data::Data;
use crate::format;
use crate::home::{self, Home};
use crate::overlay::{Confirm, Help, Menu, Outcome, Overlay, Palette};
use crate::theme::Theme;
use crate::ui::{self, Canvas, Cx, Hits, Target, text};

/// How long a banner stays up.
const BANNER_TTL: Duration = Duration::from_secs(4);
/// Two clicks on the same spot within this are a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(350);

/// Something only the app can do, asked for by a screen, overlay or click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    Banner(Banner),
    Launch {
        id: String,
        offline: bool,
    },
    NewInstance,
    Settings,
    Accounts,
    Mods(String),
    ResourcePacks(String),
    EditInstance(String),
    OpenFolder(String),
    /// Ask before deleting.
    ConfirmDelete(String),
    Delete(String),
    /// Jump to an instance.
    Select(String),
    ActionsMenu {
        id: String,
        anchor: Option<Position>,
    },
}

impl Action {
    pub fn launch(inst: &Instance, offline: bool) -> Self {
        Self::Launch {
            id: inst.id.clone(),
            offline,
        }
    }
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

#[derive(Debug)]
pub struct App {
    data: Data,
    theme: Theme,
    home: Home,
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
    splash: &'static str,
    /// Opens a folder in the file manager (swapped out in tests).
    open: fn(&std::path::Path) -> std::io::Result<()>,
}

impl App {
    pub fn new(data: Data, notices: Vec<String>, theme: Theme, size: Rect) -> Self {
        let home = Home::new(&data);
        let mut app = Self {
            data,
            theme,
            home,
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
            splash: splash(),
            open: |path| open::that_detached(path),
        };
        app.relayout();
        app
    }

    #[cfg(test)]
    pub fn set_splash(&mut self, splash: &'static str) {
        self.splash = splash;
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

    /// When the app next needs to wake without input.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.banner.as_ref().map(|(_, until)| *until)
    }

    /// Time passed. Returns whether to redraw.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.banner.as_ref().is_some_and(|(_, until)| *until <= now) {
            self.banner = None;
            self.relayout();
            return true;
        }
        false
    }

    /// Handles one terminal event. Returns whether to redraw.
    pub fn handle(&mut self, event: Event) -> bool {
        let redraw = match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.on_key(key),
            Event::Mouse(mouse) => self.on_mouse(mouse),
            Event::Paste(text) => self.on_paste(&text),
            Event::Resize(w, h) => {
                self.size = Rect::new(0, 0, w, h);
                true
            }
            Event::FocusGained => {
                // Pick up instances added or edited while we were away.
                self.reload();
                true
            }
            Event::Key(_) | Event::FocusLost => false,
        };
        if redraw {
            self.relayout();
        }
        redraw
    }

    fn on_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
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

        if !self.home.captures_text()
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            match key.code {
                KeyCode::Char('?') => {
                    self.overlay = Some(Overlay::Help(Help::new("Home", Home::help())));
                    return true;
                }
                KeyCode::Char(':') => {
                    self.overlay = Some(Overlay::Palette(Palette::new(
                        self.home.palette_items(&self.data),
                    )));
                    return true;
                }
                _ => {}
            }
        }
        match self.home.handle_key(key, &self.data) {
            home::Handled::Ignored => dismissed,
            home::Handled::Consumed => true,
            home::Handled::Action(action) => {
                self.run(action);
                true
            }
        }
    }

    fn on_paste(&mut self, text: &str) -> bool {
        match &mut self.overlay {
            Some(overlay) => {
                overlay.handle_paste(text);
                true
            }
            None => self.home.handle_paste(text, &self.data),
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
                    (None, Some(Target::Home(t))) => {
                        self.on_home_mouse(&t, home::Mouse::Scroll { down })
                    }
                    (None, _) => false,
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
        if double && !matches!(target, Target::Home(home::Target::Row(_))) {
            return dismissed;
        }
        let acted = match target {
            Target::Key(code) if left => {
                // A clicked letter is a command, not something to type.
                if matches!(code, KeyCode::Char(_)) {
                    self.home.commit_filter(&self.data);
                }
                self.on_key(KeyEvent::from(code))
            }
            Target::Home(t) if left => self.on_home_mouse(&t, home::Mouse::Click { double }),
            Target::Home(t) if button == MouseButton::Right => {
                self.on_home_mouse(&t, home::Mouse::RightClick(pos))
            }
            _ => false,
        };
        acted || dismissed
    }

    fn on_home_mouse(&mut self, target: &home::Target, mouse: home::Mouse) -> bool {
        match self.home.handle_mouse(target, mouse, &self.data) {
            home::Handled::Ignored => false,
            home::Handled::Consumed => true,
            home::Handled::Action(action) => {
                self.run(action);
                true
            }
        }
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
        let not_yet = |what: &str| Banner::info(format!("{what} isn't in the Rust build yet."));
        match action {
            Action::Quit => self.quit = true,
            Action::Banner(banner) => self.show(banner),
            Action::Launch { .. } => self.show(not_yet("Launching")),
            Action::NewInstance => self.show(not_yet("Creating instances")),
            Action::Settings => self.show(not_yet("Settings")),
            Action::Accounts => self.show(not_yet("Signing in")),
            Action::Mods(_) => self.show(not_yet("The mods browser")),
            Action::ResourcePacks(_) => self.show(not_yet("Resource packs")),
            Action::EditInstance(_) => self.show(not_yet("Editing instances")),
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
    }

    fn open_folder(&mut self, id: &str) {
        let Some(inst) = self.data.instance(id) else {
            return;
        };
        let dir = inst.game_dir();
        let banner = match std::fs::create_dir_all(&dir).and_then(|()| (self.open)(&dir)) {
            Ok(()) => Banner::info(format!("Opened “{}” in your file manager.", inst.name)),
            Err(e) => Banner::warn(format!("Couldn't open {}: {e}", dir.display())),
        };
        self.show(banner);
    }

    fn confirm_delete(&mut self, id: &str) {
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
        let dialog = Confirm::new(
            "Delete instance?",
            format!("Delete “{}”?\n{what}", inst.name),
            "Delete",
            Action::Delete(id.to_owned()),
        )
        .danger("This cannot be undone.");
        self.overlay = Some(Overlay::Confirm(dialog));
    }

    fn delete(&mut self, id: &str) {
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
    /// screen its new body.
    fn relayout(&mut self) {
        // A little air around everything: 1 row top and bottom, 2 columns each side.
        let area = self.size;
        let inner = if area.width >= 12 && area.height >= 6 {
            Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2)
        } else {
            area
        };
        self.footer = self.home.hints(&self.data);
        self.notice_lines = notice_lines(&self.notices, inner.width);
        let footer_h = ui::hints_height(&self.footer, inner.width);
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
        self.home.set_body(self.chrome.body);
        // The body may have changed the footer (a detail pane that no longer fits).
        self.footer = self.home.hints(&self.data);
    }

    /// Draws a frame. Returns its clickable regions and where the cursor goes.
    pub fn render(&self, frame: &mut Frame<'_>, cx: &Cx) -> (Hits, Option<Position>) {
        let area = frame.area();
        let mut cv = Canvas::new(frame.buffer_mut(), cx);
        let chrome = self.chrome;
        let t = self.theme;

        let account = self.account_status();
        cv.header(
            chrome.header,
            &["Instances"],
            Some((account, Target::Key(KeyCode::Char('a')))),
        );
        self.home.render(&mut cv, &self.data, self.splash);

        let lines = &self.notice_lines;
        for (i, line) in lines.iter().enumerate() {
            let last = i + 1 == lines.len();
            let fg = if last { t.muted } else { t.warning };
            cv.text(
                chrome.notices.x,
                chrome.notices.y + i as u16,
                line,
                chrome.notices.width,
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
        let t = self.theme;
        match self.data.accounts.active() {
            None => Line::styled("Not signed in", Style::new().fg(t.dim)),
            Some(acc) => {
                let name = Span::styled(acc.name.clone(), Style::new().fg(t.text));
                match acc.kind {
                    AccountKind::Msa => Line::from(vec![
                        Span::styled(format!("{} ", ui::DOT), Style::new().fg(t.success)),
                        name,
                    ]),
                    AccountKind::Offline => Line::from(vec![
                        name,
                        Span::styled(" · offline", Style::new().fg(t.dim)),
                    ]),
                    AccountKind::Other(_) => Line::from(name),
                }
            }
        }
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
mod tests;
