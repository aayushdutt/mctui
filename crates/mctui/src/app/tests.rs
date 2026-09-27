//! App-level tests: whole frames (snapshots) and input end to end.

use crossterm::event::{Event as TermEvent, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tempfile::TempDir;

use super::*;
use crate::home;
use crate::task::Tasks;
use crate::testing;
use crate::theme;

/// Drives a whole app in tests: keys and clicks in, frames out.
pub(crate) struct Harness {
    pub app: App,
    _dir: TempDir,
    size: (u16, u16),
    /// Stands in for the online services in [`Harness::online`] tests.
    server: Option<wiremock::MockServer>,
    runtime: tokio::runtime::Runtime,
    events: tokio::sync::mpsc::UnboundedReceiver<Event>,
}

impl Harness {
    pub fn new(w: u16, h: u16) -> Self {
        Self::with(w, h, |_| {})
    }

    pub fn with(w: u16, h: u16, edit: impl FnOnce(&mut Data)) -> Self {
        Self::with_notices(w, h, Vec::new(), edit)
    }

    fn with_notices(w: u16, h: u16, notices: Vec<String>, edit: impl FnOnce(&mut Data)) -> Self {
        Self::build(w, h, notices, edit, false)
    }

    /// A harness whose online services are a mock server: give it answers
    /// with [`Harness::mock`]. Otherwise nothing online is reachable.
    pub fn online(w: u16, h: u16, edit: impl FnOnce(&mut Data)) -> Self {
        Self::build(w, h, Vec::new(), edit, true)
    }

    fn build(
        w: u16,
        h: u16,
        notices: Vec<String>,
        edit: impl FnOnce(&mut Data),
        online: bool,
    ) -> Self {
        let testing::Fixture { dir, mut data } = testing::fixture();
        edit(&mut data);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (tx, events) = tokio::sync::mpsc::unbounded_channel();
        let tasks = Tasks::new(tx, runtime.handle().clone());
        let server = online.then(|| runtime.block_on(wiremock::MockServer::start()));
        let services = testing::services(server.as_ref().map(wiremock::MockServer::uri), &data.dir);
        let terminal = crate::theme::Terminal::default();
        let mut app = App::new(
            data,
            notices,
            terminal,
            (tasks, services),
            Rect::new(0, 0, w, h),
        );
        app.set_splash("Blazingly monospaced!");
        app.open = |_| Ok(());
        let mut h = Self {
            app,
            _dir: dir,
            size: (w, h),
            server,
            runtime,
            events,
        };
        h.draw();
        h
    }

    /// Mounts an answer on the mock server.
    pub fn mock(&self, mock: wiremock::Mock) {
        let server = self.server.as_ref().expect("an online harness");
        self.runtime.block_on(mock.mount(server));
    }

    /// How many requests the mock server has had for `path`.
    pub fn requests(&self, path: &str) -> usize {
        let server = self.server.as_ref().expect("an online harness");
        let requests = self.runtime.block_on(server.received_requests());
        requests
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path() == path)
            .count()
    }

    pub fn data(&self) -> &Data {
        &self.app.data
    }

    pub fn app_theme(&self) -> &'static str {
        self.app.theme.name
    }

    /// Waits for background work to report back, then handles what arrived.
    #[allow(dead_code)]
    pub fn settle(&mut self) {
        let Self {
            runtime,
            events,
            app,
            ..
        } = self;
        runtime.block_on(async {
            while let Ok(Some(event)) =
                tokio::time::timeout(std::time::Duration::from_millis(200), events.recv()).await
            {
                app.handle(event);
            }
        });
        self.draw();
    }

    pub fn draw(&mut self) -> Buffer {
        let (w, h) = self.size;
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let cx = self.app.cx(testing::now(), testing::tz());
        let mut hits = None;
        terminal
            .draw(|f| hits = Some(self.app.render(f, &cx).0))
            .unwrap();
        self.app.set_hits(hits.unwrap());
        terminal.backend().buffer().clone()
    }

    pub fn text(&mut self) -> String {
        testing::text(&self.draw())
    }

    pub fn press(&mut self, code: KeyCode) -> bool {
        let redraw = self
            .app
            .handle(Event::Term(TermEvent::Key(KeyEvent::from(code))));
        self.draw();
        redraw
    }

    fn ctrl(&mut self, c: char) {
        let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        self.app.handle(Event::Term(TermEvent::Key(key)));
        self.draw();
    }

    pub fn type_text(&mut self, s: &str) {
        for c in s.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    pub fn mouse(&mut self, kind: MouseEventKind, at: Position) -> bool {
        let event = MouseEvent {
            kind,
            column: at.x,
            row: at.y,
            modifiers: KeyModifiers::NONE,
        };
        let redraw = self.app.handle(Event::Term(TermEvent::Mouse(event)));
        self.draw();
        redraw
    }

    /// Where `target` was drawn in the last frame.
    pub fn find(&self, target: &Target) -> Position {
        let area = self
            .app
            .hits
            .find(target)
            .unwrap_or_else(|| panic!("{target:?} isn't on screen"));
        area.as_position()
    }

    pub fn click(&mut self, target: &Target) -> bool {
        let at = self.find(target);
        // Keep separate clicks from reading as a double-click.
        self.app.last_click = None;
        self.mouse(MouseEventKind::Down(MouseButton::Left), at)
    }

    fn selected(&self) -> Option<&str> {
        self.app
            .home
            .selected_instance(&self.app.data)
            .map(|i| i.name.as_str())
    }

    /// Whether the footer currently offers `label`.
    fn offers(&self, label: &str) -> bool {
        self.app
            .home
            .hints(&self.app.data)
            .iter()
            .any(|h| h.label == label)
    }

    pub fn banner(&self) -> Option<&str> {
        self.app.banner.as_ref().map(|(b, _)| b.text.as_str())
    }
}

/// A list row by instance name (the fixture's ids are its names).
fn row(name: &str) -> Target {
    Target::Home(home::Target::Row(home::RowKey::Instance(name.to_owned())))
}

#[test]
fn home_80x24() {
    insta::assert_snapshot!(Harness::new(80, 24).text());
}

#[test]
fn home_wide() {
    insta::assert_snapshot!(Harness::new(120, 32).text());
}

#[test]
fn home_compact() {
    insta::assert_snapshot!(Harness::new(60, 24).text());
}

#[test]
fn home_empty() {
    let mut h = Harness::with(80, 24, |d| d.instances.clear());
    insta::assert_snapshot!(h.text());
}

#[test]
fn home_detail_tabs() {
    let mut h = Harness::new(100, 28);
    h.press(KeyCode::Right);
    h.press(KeyCode::Right);
    insta::assert_snapshot!(h.text());
}

#[test]
fn help_card() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Char('?'));
    insta::assert_snapshot!(h.text());
    h.press(KeyCode::Char('q'));
    assert!(h.app.overlay.is_none(), "q closes help");
    assert!(!h.app.should_quit());
}

#[test]
fn palette_filters_and_runs() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Char(':'));
    insta::assert_snapshot!(h.text());
    h.type_text("creat");
    insta::assert_snapshot!("palette_filtered", h.text());
    h.press(KeyCode::Enter);
    assert!(h.app.overlay.is_none());
    assert_eq!(
        h.selected(),
        Some("Creative Build"),
        "jumped to the instance"
    );
}

#[test]
fn delete_opens_on_cancel() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Char('d'));
    insta::assert_snapshot!(h.text());
    h.press(KeyCode::Enter);
    assert!(h.app.overlay.is_none());
    assert_eq!(
        h.app.data.instances.len(),
        3,
        "enter on the default choice keeps it"
    );
}

#[test]
fn delete_confirmed_removes_the_folder() {
    let mut h = Harness::new(80, 24);
    let path = h.app.data.instances[0].path.clone();
    assert!(path.exists());
    h.press(KeyCode::Char('d'));
    h.press(KeyCode::Left);
    h.press(KeyCode::Enter);
    assert!(!path.exists());
    assert_eq!(h.app.data.instances.len(), 2);
    assert_eq!(h.banner(), Some("Deleted “Survival 1.21”."));
}

#[test]
fn right_click_opens_the_actions_menu_at_the_pointer() {
    let mut h = Harness::new(80, 24);
    let at = h.find(&row("Creative Build"));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    assert_eq!(h.selected(), Some("Creative Build"));
    insta::assert_snapshot!(h.text());
    // Clicking outside the card closes it.
    h.mouse(MouseEventKind::Down(MouseButton::Left), Position::new(0, 0));
    assert!(h.app.overlay.is_none());
}

#[test]
fn clicks_select_focus_and_act() {
    let mut h = Harness::new(100, 28);
    h.click(&row("Snapshot Test"));
    assert_eq!(h.selected(), Some("Snapshot Test"));

    h.click(&Target::Home(home::Target::Tab(home::Tab::Mods)));
    assert!(h.text().contains("Mods 0"));
    assert_eq!(h.app.home.hints(&h.app.data)[1].label, "open");

    // Double-click plays.
    let at = h.find(&row("Survival 1.21"));
    h.app.last_click = None;
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    assert_eq!(h.banner(), Some("Launching isn't in the Rust build yet."));

    // Footer hints are buttons.
    h.click(&Target::Key(KeyCode::Char('n')));
    assert_eq!(
        h.banner(),
        Some("Creating instances isn't in the Rust build yet.")
    );
}

#[test]
fn wheel_moves_the_selection_under_the_pointer() {
    let mut h = Harness::new(80, 24);
    let at = h.find(&row("Survival 1.21"));
    h.mouse(MouseEventKind::ScrollDown, at);
    assert_eq!(h.selected(), Some("Snapshot Test"));
    h.mouse(MouseEventKind::ScrollUp, at);
    h.mouse(MouseEventKind::ScrollUp, at);
    assert_eq!(h.selected(), Some("Survival 1.21"), "stops at the top");
}

#[test]
fn hover_redraws_only_when_the_target_changes() {
    let mut h = Harness::new(80, 24);
    let at = h.find(&row("Snapshot Test"));
    assert!(h.mouse(MouseEventKind::Moved, at));
    assert!(!h.mouse(MouseEventKind::Moved, Position::new(at.x + 1, at.y)));
    assert_eq!(h.app.hover, Some(row("Snapshot Test")));
}

#[test]
fn arrows_stay_in_their_pane_and_stop_at_the_ends() {
    let mut h = Harness::new(100, 28);
    assert!(!h.press(KeyCode::Up), "already at the top");
    for _ in 0..10 {
        h.press(KeyCode::Down);
    }
    assert_eq!(h.selected(), None, "stopped on + New instance");
    h.press(KeyCode::Char('g'));
    assert_eq!(h.selected(), Some("Survival 1.21"));

    h.press(KeyCode::Right);
    assert!(h.offers("tabs"), "detail pane focused");
    h.press(KeyCode::Down);
    assert_eq!(
        h.selected(),
        Some("Survival 1.21"),
        "↓ doesn't move the hidden list"
    );
    h.press(KeyCode::Esc);
    assert!(h.offers("play"), "back on the list");
}

#[test]
fn filter_narrows_and_esc_clears() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Char('/'));
    h.type_text("creq");
    assert!(!h.app.should_quit(), "q types into the filter");
    h.press(KeyCode::Backspace);
    let text = h.text();
    assert!(
        text.contains("Creative Build") && !text.contains("Survival"),
        "{text}"
    );
    h.press(KeyCode::Enter);
    assert_eq!(h.selected(), Some("Creative Build"));
    assert!(h.offers("clear filter"));
    h.press(KeyCode::Esc);
    assert!(h.text().contains("Survival"));
    assert_eq!(h.selected(), Some("Creative Build"), "selection survives");
}

#[test]
fn quitting() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Esc);
    assert_eq!(h.banner(), Some("Press q to quit."));
    h.press(KeyCode::Char('q'));
    assert!(h.app.should_quit());

    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Char(':'));
    h.ctrl('c');
    assert!(h.app.should_quit(), "ctrl+c quits from anywhere");
}

#[test]
fn every_documented_key_does_something() {
    let keys = [
        KeyCode::Down,
        KeyCode::Char('j'),
        KeyCode::Up,
        KeyCode::Char('k'),
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::Tab,
        KeyCode::Right,
        KeyCode::Char('l'),
        KeyCode::Char(']'),
        KeyCode::Left,
        KeyCode::Char('h'),
        KeyCode::Char('['),
        KeyCode::Enter,
        KeyCode::Char('o'),
        KeyCode::Char('n'),
        KeyCode::Char('e'),
        KeyCode::Char('d'),
        KeyCode::Char('f'),
        KeyCode::Char('m'),
        KeyCode::Char('p'),
        KeyCode::Char('s'),
        KeyCode::Char('a'),
        KeyCode::Char('x'),
        KeyCode::Char(':'),
        KeyCode::Char('/'),
        KeyCode::Esc,
        KeyCode::Char('?'),
        KeyCode::Char('q'),
    ];
    for code in keys {
        let mut h = Harness::new(100, 28);
        // Start one row down so movement both ways does something.
        h.press(KeyCode::Down);
        if matches!(code, KeyCode::Left | KeyCode::Char('h' | '[')) {
            h.press(KeyCode::Tab);
            h.press(KeyCode::Right);
        }
        let before = h.text();
        let reacted = h.press(code);
        let changed = h.text() != before || h.banner().is_some() || h.app.should_quit();
        assert!(reacted && changed, "{code:?} did nothing");
    }
}

#[test]
fn startup_notices_show_until_a_key() {
    let notice = "Couldn't read the instance in “Broken”: bad JSON".to_owned();
    let mut h = Harness::with_notices(80, 24, vec![notice], |_| {});
    assert!(h.text().contains("press any key to dismiss"));
    h.press(KeyCode::Down);
    assert!(!h.text().contains("press any key"));
    assert_eq!(
        h.selected(),
        Some("Snapshot Test"),
        "the key still did its job"
    );
}

#[test]
fn never_panics_at_any_size() {
    for w in (0..=130).step_by(7) {
        for hgt in (0..=40).step_by(3) {
            let mut h = Harness::new(w, hgt);
            h.press(KeyCode::Right);
            for code in [
                KeyCode::Char('?'),
                KeyCode::Esc,
                KeyCode::Char(':'),
                KeyCode::Esc,
                KeyCode::Char('d'),
            ] {
                h.press(code);
            }
            h.press(KeyCode::Esc);
            h.press(KeyCode::Char('x'));
        }
    }
}

/// Writes ANSI renders of the main states to `$MCTUI_SCREENSHOTS` for
/// `scripts/ansi2png.py`. Run with `--ignored`.
#[test]
#[ignore = "writes screenshots when MCTUI_SCREENSHOTS is set"]
fn screenshots() {
    let Some(dir) = std::env::var_os("MCTUI_SCREENSHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let save = |name: &str, h: &mut Harness| {
        std::fs::write(dir.join(format!("{name}.ansi")), testing::ansi(&h.draw())).unwrap();
    };

    let mut h = Harness::new(100, 30);
    save("home", &mut h);
    let at = h.find(&row("Snapshot Test"));
    h.mouse(MouseEventKind::Moved, at);
    save("home-hover", &mut h);
    h.press(KeyCode::Tab);
    h.press(KeyCode::Right);
    save("home-detail-mods", &mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('?'));
    save("help", &mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char(':'));
    h.type_text("pl");
    save("palette", &mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('d'));
    save("delete", &mut h);
    h.press(KeyCode::Esc);
    let at = h.find(&row("Creative Build"));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    save("context-menu", &mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    h.type_text("sur");
    save("filter", &mut h);

    save("home-80x24", &mut Harness::new(80, 24));
    save("home-compact", &mut Harness::new(64, 24));
    save(
        "empty",
        &mut Harness::with(100, 30, |d| d.instances.clear()),
    );
    let mut light = Harness::new(100, 30);
    light.app.theme = crate::theme::Theme::new(&theme::LIGHT, None);
    save("home-light", &mut light);

    let mut settings = Harness::new(100, 30);
    settings.press(KeyCode::Char('s'));
    save("settings", &mut settings);
    settings.press(KeyCode::Down);
    save("settings-toggle", &mut settings);

    let light = |h: &mut Harness| h.app.theme = crate::theme::Theme::new(&theme::LIGHT, None);
    crate::accounts::tests::screenshots(&save, &light);
}

// Regressions from the phase-1 review.

#[test]
fn a_reload_keeps_the_selected_instance() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Down);
    assert_eq!(h.selected(), Some("Snapshot Test"));
    // A newer instance appears on disk and sorts first.
    let draft = mctui_core::instance::NewInstance {
        name: "Brand New".into(),
        version: "1.21.4".into(),
        ..Default::default()
    };
    h.app.data.store.create(draft).unwrap();
    h.app.handle(Event::Term(TermEvent::FocusGained));
    assert_eq!(h.selected(), Some("Snapshot Test"));
}

#[test]
fn a_click_hits_what_was_on_screen_even_if_the_list_moved() {
    let mut h = Harness::new(80, 24);
    let at = h.find(&row("Snapshot Test"));
    let draft = mctui_core::instance::NewInstance {
        name: "Brand New".into(),
        version: "1.21.4".into(),
        ..Default::default()
    };
    h.app.data.store.create(draft).unwrap();
    // Focus and the click arrive together, before the next frame.
    h.app.handle(Event::Term(TermEvent::FocusGained));
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: at.x,
        row: at.y,
        modifiers: KeyModifiers::NONE,
    };
    h.app.handle(Event::Term(TermEvent::Mouse(click)));
    assert_eq!(h.selected(), Some("Snapshot Test"));
}

#[test]
fn clicks_on_different_things_are_not_a_double_click() {
    let mut h = Harness::new(80, 24);
    let at = h.find(&row("Creative Build"));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    assert!(h.app.overlay.is_some());
    // Close the menu by clicking where the row was, then click the row.
    h.mouse(MouseEventKind::Down(MouseButton::Left), Position::new(0, 0));
    let at = h.find(&row("Creative Build"));
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    assert_eq!(h.banner(), None, "no launch");

    let play = h.find(&Target::Home(home::Target::Play));
    h.app.last_click = None;
    h.mouse(MouseEventKind::Down(MouseButton::Left), play);
    h.app.banner = None;
    h.mouse(MouseEventKind::Down(MouseButton::Left), play);
    assert_eq!(h.banner(), None, "a button acts once per double-click");
}

#[test]
fn hovering_delete_never_arms_enter() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Char('d'));
    let yes = h.find(&Target::Overlay(crate::overlay::Target::Yes));
    h.mouse(MouseEventKind::Moved, yes);
    h.mouse(MouseEventKind::Moved, Position::new(0, 0));
    h.press(KeyCode::Enter);
    assert_eq!(h.app.data.instances.len(), 3);
}

#[test]
fn clicking_a_key_hint_while_typing_runs_the_command() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Char('/'));
    h.type_text("sur");
    h.click(&Target::Key(KeyCode::Char('a')));
    assert!(h.text().contains("ACCOUNTS"), "opened the accounts");
    h.press(KeyCode::Esc);
    assert!(h.text().contains("“sur”"), "the filter kept what was typed");
}

#[test]
fn huge_terminals_are_fine() {
    for (w, hgt) in [(2000, 20), (65535, 8), (120, 3000)] {
        Harness::new(w, hgt);
    }
}

#[test]
fn short_terminals_keep_panes_out_of_the_footer() {
    let mut h = Harness::new(80, 10);
    h.press(KeyCode::Tab);
    let text = h.text();
    let footer = text
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or_default();
    assert!(
        footer.contains("tabs") && !footer.contains("Overview"),
        "{text}"
    );

    // A confirmation on a short screen keeps its buttons.
    let mut h = Harness::new(60, 12);
    h.press(KeyCode::Char('d'));
    let text = h.text();
    assert!(text.contains("Delete") && text.contains("Cancel"), "{text}");
}

#[test]
fn help_scrolls_when_it_does_not_fit() {
    let mut h = Harness::new(50, 16);
    h.press(KeyCode::Char('?'));
    let before = h.text();
    assert!(before.contains("scroll"), "{before}");
    h.press(KeyCode::Down);
    assert_ne!(h.text(), before);
    h.press(KeyCode::Char('q'));
    assert!(h.app.overlay.is_none());
}
