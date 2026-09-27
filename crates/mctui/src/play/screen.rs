//! The launch screen: the steps while Minecraft gets ready, then the game's
//! log while it runs, then how it ended.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mctui_core::config::LogVerbosity;
use mctui_core::java::InstallProgress;
use mctui_core::launch::{Exit, Level, LogLine, Stage, Step};
use mctui_core::modrinth::STARTER_MODS;
use mctui_core::net::Progress;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::{Failure, Phase, Session};
use crate::app::{Action, Banner, Question, Route};
use crate::data::Data;
use crate::format;
use crate::overlay::HelpGroup;
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::theme::Theme;
use crate::ui::widgets::{self, ButtonKind, Tone};
use crate::ui::{self, Canvas, Hint, Target, text};

/// The launch screen's clickable parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayTarget {
    /// The log: the wheel scrolls it.
    Log,
    Retry,
    Offline,
    SignIn,
    Stop,
    Copy,
}

/// The stages shown while launching, and what they're called.
const STAGES: [(Stage, &str); 7] = [
    (Stage::Version, "Game version"),
    (Stage::Java, "Java"),
    (Stage::StarterMods, "Starter mods"),
    (Stage::Libraries, "Game files"),
    (Stage::Assets, "Assets"),
    (Stage::Natives, "Native libraries"),
    (Stage::Starting, "Starting Minecraft"),
];

#[derive(Debug)]
pub struct PlayScreen {
    id: String,
    body: Rect,
    /// Log lines scrolled up from the newest; 0 follows the log as it grows.
    scroll: usize,
}

impl PlayScreen {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            body: Rect::default(),
            scroll: 0,
        }
    }

    fn session<'d>(&self, data: &'d Data) -> Option<&'d Session> {
        data.game.as_ref().filter(|g| g.instance_id == self.id)
    }

    fn relaunch(&self, offline: bool) -> Handled {
        Handled::Action(Action::Open(Route::Launch {
            id: self.id.clone(),
            offline,
        }))
    }

    fn sign_in(&self) -> Handled {
        let then = Route::Launch {
            id: self.id.clone(),
            offline: false,
        };
        Handled::Action(Action::Open(Route::SignIn {
            then: Some(Box::new(then)),
        }))
    }

    fn stop() -> Handled {
        Handled::Action(Action::Confirm(Box::new(Question {
            title: "Stop Minecraft?".into(),
            message:
                "The game saves your world and quits. If it doesn't within 15 seconds, it's closed."
                    .into(),
            yes: "Stop",
            danger: None,
            on_yes: Action::StopGame,
        })))
    }

    fn copy_log(&self, data: &Data) -> Handled {
        let Some(session) = self.session(data) else {
            return Handled::Ignored;
        };
        let verbosity = data.config.launch_log_verbosity;
        let text: Vec<&str> = shown(session, verbosity).map(|l| l.text.as_str()).collect();
        Handled::Action(Action::Many(vec![
            Action::Copy(text.join("\n")),
            Action::Banner(Banner::info(format!("Copied {} log lines.", text.len()))),
        ]))
    }

    fn scroll_by(&mut self, delta: isize, data: &Data) -> bool {
        let total = self
            .session(data)
            .map_or(0, |s| shown(s, data.config.launch_log_verbosity).count());
        let next = self
            .scroll
            .saturating_add_signed(delta)
            .min(total.saturating_sub(1));
        std::mem::replace(&mut self.scroll, next) != next
    }

    fn cycle_verbosity(cx: &mut Ctx<'_>) -> Handled {
        let config = &mut cx.data.config;
        config.launch_log_verbosity = match config.launch_log_verbosity {
            LogVerbosity::Error => LogVerbosity::Warn,
            LogVerbosity::Warn => LogVerbosity::All,
            LogVerbosity::All => LogVerbosity::Error,
        };
        match config.save(&cx.data.dir) {
            Ok(()) => Handled::Consumed,
            Err(e) => Handled::Action(Action::Banner(Banner::warn(format!(
                "Couldn't save the log setting: {e}"
            )))),
        }
    }
}

/// The log lines shown at `verbosity`, oldest first.
fn shown(session: &Session, verbosity: LogVerbosity) -> impl DoubleEndedIterator<Item = &LogLine> {
    session
        .log
        .iter()
        .filter(move |l| l.level.shown_at(verbosity))
}

fn verbosity_label(v: LogVerbosity) -> &'static str {
    match v {
        LogVerbosity::Error => "errors only",
        LogVerbosity::Warn => "warnings and errors",
        LogVerbosity::All => "everything",
    }
}

impl Screen for PlayScreen {
    fn crumbs(&self, data: &Data) -> Vec<String> {
        let name = data
            .instance(&self.id)
            .map_or_else(|| self.id.clone(), |i| i.name.clone());
        vec![name, "Play".into()]
    }

    fn hints(&self, data: &Data) -> Vec<Hint> {
        let key = Hint::key;
        match self.session(data).map(|s| &s.phase) {
            Some(Phase::Preparing(_)) => vec![key(KeyCode::Esc, "cancel")],
            Some(Phase::Playing { .. }) => vec![
                key(KeyCode::Char('s'), "stop"),
                key(KeyCode::Char('v'), "log level"),
                key(KeyCode::Char('c'), "copy log"),
                key(KeyCode::Esc, "back · keeps playing"),
            ],
            Some(Phase::Ended(_)) => vec![
                key(KeyCode::Enter, "play again"),
                key(KeyCode::Char('c'), "copy log"),
                key(KeyCode::Char('v'), "log level"),
                key(KeyCode::Esc, "back"),
            ],
            Some(Phase::Failed(Failure::SignIn(_))) => vec![
                key(KeyCode::Enter, "sign in"),
                key(KeyCode::Char('o'), "play offline"),
                key(KeyCode::Esc, "back"),
            ],
            Some(Phase::Failed(Failure::Launch(_))) => vec![
                key(KeyCode::Char('r'), "try again"),
                key(KeyCode::Char('o'), "play offline"),
                key(KeyCode::Esc, "back"),
            ],
            Some(Phase::Stopping) | None => vec![key(KeyCode::Esc, "back")],
        }
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        let groups = vec![
            HelpGroup {
                title: "Game",
                entries: &[
                    ("s", "stop the game"),
                    ("↵", "play again · sign in"),
                    ("r", "try again"),
                    ("o", "play offline"),
                ],
            },
            HelpGroup {
                title: "Log",
                entries: &[
                    ("↑↓ j k", "scroll"),
                    ("pgup pgdn", "page"),
                    ("G end", "follow new lines"),
                    ("v", "errors · warnings · everything"),
                    ("c", "copy the log"),
                ],
            },
            HelpGroup {
                title: "General",
                entries: &[("esc", "back (the game keeps running)"), ("?", "help")],
            },
        ];
        ("Play", groups)
    }

    fn set_body(&mut self, body: Rect) {
        self.body = body;
    }

    fn plays(&self, id: &str) -> bool {
        self.id == id
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Handled::Ignored;
        }
        let page = isize::try_from(self.body.height.saturating_sub(8).max(1)).unwrap_or(1);
        let phase = self.session(cx.data).map(|s| &s.phase);
        match (key.code, phase) {
            (KeyCode::Esc, Some(Phase::Preparing(_))) => {
                // Cancelling drops the launch; nothing half-done is left.
                cx.data.game = None;
                Handled::Action(Action::Many(vec![
                    Action::Close,
                    Action::Banner(Banner::info("Launch cancelled.")),
                ]))
            }
            (KeyCode::Esc, _) => Handled::Action(Action::Close),
            (KeyCode::Char('s'), Some(Phase::Playing { .. })) => Self::stop(),
            (
                KeyCode::Char('v'),
                Some(Phase::Playing { .. } | Phase::Stopping | Phase::Ended(_)),
            ) => {
                self.scroll = 0;
                Self::cycle_verbosity(cx)
            }
            (
                KeyCode::Char('c'),
                Some(Phase::Playing { .. } | Phase::Stopping | Phase::Ended(_)),
            ) => self.copy_log(cx.data),
            (KeyCode::Enter, Some(Phase::Ended(_))) => {
                self.relaunch(self.session(cx.data).is_some_and(|s| s.offline))
            }
            (KeyCode::Enter, Some(Phase::Failed(Failure::SignIn(_)))) => self.sign_in(),
            (KeyCode::Enter | KeyCode::Char('r'), Some(Phase::Failed(Failure::Launch(_)))) => {
                self.relaunch(self.session(cx.data).is_some_and(|s| s.offline))
            }
            (KeyCode::Char('o'), Some(Phase::Failed(_))) => self.relaunch(true),
            (KeyCode::Up | KeyCode::Char('k'), _) => Handled::changed(self.scroll_by(1, cx.data)),
            (KeyCode::Down | KeyCode::Char('j'), _) => {
                Handled::changed(self.scroll_by(-1, cx.data))
            }
            (KeyCode::PageUp, _) => Handled::changed(self.scroll_by(page, cx.data)),
            (KeyCode::PageDown, _) => Handled::changed(self.scroll_by(-page, cx.data)),
            (KeyCode::End | KeyCode::Char('G'), _) => {
                Handled::changed(std::mem::take(&mut self.scroll) != 0)
            }
            (KeyCode::Home | KeyCode::Char('g'), _) => {
                Handled::changed(self.scroll_by(isize::MAX, cx.data))
            }
            _ => Handled::Ignored,
        }
    }

    fn handle_mouse(&mut self, target: &Target, mouse: Mouse, cx: &mut Ctx<'_>) -> Handled {
        let Target::Play(target) = target else {
            return Handled::Ignored;
        };
        match (target, mouse) {
            (PlayTarget::Log, Mouse::Scroll { down }) => {
                Handled::changed(self.scroll_by(if down { -3 } else { 3 }, cx.data))
            }
            (PlayTarget::Retry, Mouse::Click { .. }) => {
                self.relaunch(self.session(cx.data).is_some_and(|s| s.offline))
            }
            (PlayTarget::Offline, Mouse::Click { .. }) => self.relaunch(true),
            (PlayTarget::SignIn, Mouse::Click { .. }) => self.sign_in(),
            (PlayTarget::Stop, Mouse::Click { .. }) => Self::stop(),
            (PlayTarget::Copy, Mouse::Click { .. }) => self.copy_log(cx.data),
            _ => Handled::Ignored,
        }
    }

    fn render(&self, cv: &mut Canvas<'_>, data: &Data) {
        let area = Rect {
            x: self.body.x + 2,
            width: self.body.width.saturating_sub(4),
            ..self.body
        };
        let Some(session) = self.session(data) else {
            cv.text(
                area.x,
                area.y,
                "Nothing is running.",
                area.width,
                Style::new().fg(cv.theme().muted),
            );
            return;
        };
        let y = Self::head(cv, area, session);
        let rest = Rect::new(area.x, y, area.width, area.bottom().saturating_sub(y));
        match &session.phase {
            Phase::Preparing(step) => steps(cv, rest, session, *step),
            Phase::Playing { .. } | Phase::Stopping => {
                self.log(cv, rest, session, data.config.launch_log_verbosity);
            }
            Phase::Ended(report) => {
                let y = ended(cv, rest, report);
                let rest = Rect::new(area.x, y, area.width, area.bottom().saturating_sub(y));
                self.log(cv, rest, session, data.config.launch_log_verbosity);
            }
            Phase::Failed(failure) => failed(cv, rest, failure, session.offline),
        }
    }
}

impl PlayScreen {
    /// The instance, who's playing, and the state chip. Returns the next row.
    fn head(cv: &mut Canvas<'_>, area: Rect, session: &Session) -> u16 {
        let t = *cv.theme();
        let (chip, tone) = state_chip(session, &t);
        let chip_w = chip.width() as u16 + 2;
        let chip = widgets::chip(
            cv,
            area.right().saturating_sub(chip_w),
            area.y,
            &chip,
            tone,
            None,
            chip_w,
        );
        let room = chip.x.saturating_sub(area.x + 2);
        let name = text::truncate(&session.instance_name, usize::from(room));
        cv.text(
            area.x,
            area.y,
            &name,
            room,
            Style::new().fg(t.strong).bold(),
        );

        let who = if session.offline {
            format!("{}  ·  offline as {}", session.title, session.player)
        } else {
            format!("{}  ·  as {}", session.title, session.player)
        };
        cv.text(
            area.x,
            area.y + 1,
            &text::truncate(&who, usize::from(area.width)),
            area.width,
            Style::new().fg(t.dim),
        );
        let mut y = area.y + 2;
        for note in &session.notes {
            cv.text(area.x, y, note, area.width, Style::new().fg(t.muted));
            y += 1;
        }
        if let Phase::Playing { .. } = session.phase {
            let target = Target::Play(PlayTarget::Stop);
            widgets::button(
                cv,
                area.x,
                y + 1,
                "Stop",
                Some("s"),
                ButtonKind::Secondary,
                target,
            );
            y += 2;
        }
        y + 1
    }

    /// The game log, newest at the bottom, filtered to `verbosity`.
    fn log(&self, cv: &mut Canvas<'_>, area: Rect, session: &Session, verbosity: LogVerbosity) {
        let t = *cv.theme();
        if area.height < 3 {
            return;
        }
        let lines: Vec<&LogLine> = shown(session, verbosity).collect();
        let count = lines.len().to_string();
        widgets::section(
            cv,
            Rect { height: 1, ..area },
            "Game log",
            Some(&count),
            false,
        );
        let filter = Line::from(vec![
            Span::styled(verbosity_label(verbosity), Style::new().fg(t.subtle)),
            Span::styled("  v", Style::new().fg(t.muted)),
        ]);
        cv.line_right(area, area.y, &filter);
        cv.click(
            Rect::new(area.x, area.y, area.width, 1),
            Target::Key(KeyCode::Char('v')),
        );

        let body = Rect::new(area.x, area.y + 2, area.width, area.height - 2);
        cv.click(body, Target::Play(PlayTarget::Log));
        if lines.is_empty() {
            let empty = match verbosity {
                LogVerbosity::All => "Nothing yet.",
                _ => "No problems so far. Press v to see everything.",
            };
            cv.text(body.x, body.y, empty, body.width, Style::new().fg(t.faint));
            return;
        }
        let height = usize::from(body.height.saturating_sub(u16::from(self.scroll > 0)));
        let end = lines.len().saturating_sub(self.scroll);
        let start = end.saturating_sub(height);
        for (i, line) in lines[start..end].iter().enumerate() {
            let fg = match line.level {
                Level::Error => t.error,
                Level::Warn => t.warning,
                Level::Info => t.text,
                Level::Debug => t.muted,
            };
            let text = text::truncate(&line.text, usize::from(body.width));
            cv.text(
                body.x,
                body.y + i as u16,
                &text,
                body.width,
                Style::new().fg(fg),
            );
        }
        if self.scroll > 0 {
            let more = format!("↓ {} newer · G to follow", self.scroll);
            cv.text(
                body.x,
                body.bottom() - 1,
                &more,
                body.width,
                Style::new().fg(t.faint),
            );
        }
    }
}

/// The chip beside the instance name.
fn state_chip(session: &Session, t: &Theme) -> (Line<'static>, Tone) {
    let styled = |s: String, fg| Line::styled(s, Style::new().fg(fg));
    match &session.phase {
        Phase::Preparing(_) => (styled("Getting ready".into(), t.text), Tone::Neutral),
        Phase::Playing { since, .. } => {
            let secs = Instant::now().saturating_duration_since(*since).as_secs();
            let time = if secs < 60 {
                "just started".to_owned()
            } else {
                format::play_time(secs)
            };
            let text = format!("{} Playing · {time}", ui::DOT);
            (styled(text, t.success), Tone::Success)
        }
        Phase::Stopping => (
            styled("Saving and quitting…".into(), t.warning),
            Tone::Warning,
        ),
        Phase::Ended(report) if report.crashed() => (
            styled(format!("{} Crashed", ui::CROSS), t.warning),
            Tone::Warning,
        ),
        Phase::Ended(_) => (styled("Closed".into(), t.dim), Tone::Neutral),
        Phase::Failed(_) => (
            styled(format!("{} Didn't start", ui::CROSS), t.warning),
            Tone::Warning,
        ),
    }
}

/// The launch steps: done, the current one with its progress, then what's left.
fn steps(cv: &mut Canvas<'_>, area: Rect, session: &Session, step: Option<Step>) {
    let t = *cv.theme();
    let current = step.map(|s| s.stage());
    let spinner = widgets::spinner_at(cv.cx.now);
    let mut y = area.y;
    for (stage, label) in STAGES {
        // Starter mods only show up for a launch that installs them.
        if stage == Stage::StarterMods && current != Some(stage) && !session.done.contains(&stage) {
            continue;
        }
        if y >= area.bottom() {
            break;
        }
        let (glyph, glyph_fg, label_style) = if session.done.contains(&stage) {
            (ui::CHECK, t.success, Style::new().fg(t.dim))
        } else if current == Some(stage) {
            (spinner, t.accent, Style::new().fg(t.strong).bold())
        } else {
            (ui::PENDING, t.faint, Style::new().fg(t.faint))
        };
        cv.text(area.x, y, glyph, 1, Style::new().fg(glyph_fg));
        cv.text(area.x + 3, y, label, 22, label_style);
        if current == Some(stage)
            && let Some(step) = step
        {
            let detail = Rect::new(area.x + 26, y, area.width.saturating_sub(26), 1);
            progress(cv, detail, step);
        }
        y += 2;
    }
}

/// The current step's progress bar and what it's doing.
fn progress(cv: &mut Canvas<'_>, area: Rect, step: Step) {
    let t = *cv.theme();
    let (ratio, detail) = match step {
        Step::Libraries(p) | Step::Assets(p) => (ratio(p), files(p)),
        Step::InstallingJava(InstallProgress::Downloading { major, done, total }) => (
            done as f64 / total.max(1) as f64,
            format!(
                "Java {major} · {} of {}",
                format::size(done),
                format::size(total)
            ),
        ),
        Step::InstallingJava(InstallProgress::Extracting { major }) => {
            (1.0, format!("unpacking Java {major}"))
        }
        Step::StarterMods(p) => {
            let per = 1.0 / STARTER_MODS.len() as f64;
            (
                per * (p.index as f64 + ratio(p.download)),
                p.starter.title.to_owned(),
            )
        }
        _ => return,
    };
    let bar_w = area
        .width
        .saturating_sub(text::width(&detail) as u16 + 3)
        .min(36);
    widgets::progress(
        cv,
        Rect {
            width: bar_w,
            ..area
        },
        ratio,
    );
    cv.text(
        area.x + bar_w + 2,
        area.y,
        &detail,
        area.width.saturating_sub(bar_w + 2),
        Style::new().fg(t.muted),
    );
}

fn ratio(p: Progress) -> f64 {
    if p.bytes_total > 0 {
        p.bytes_done as f64 / p.bytes_total as f64
    } else {
        p.files_done as f64 / p.files_total.max(1) as f64
    }
}

fn files(p: Progress) -> String {
    if p.bytes_total > 0 {
        format!(
            "{} of {}",
            format::size(p.bytes_done),
            format::size(p.bytes_total)
        )
    } else {
        format!("{} of {} files", p.files_done, p.files_total)
    }
}

/// How the game ended, with what to do next. Returns the next row.
fn ended(cv: &mut Canvas<'_>, area: Rect, report: &mctui_core::launch::ExitReport) -> u16 {
    let t = *cv.theme();
    let played = format::play_time(report.ran_for.as_secs());
    let (message, fg) = if report.crashed() {
        let how = match report.exit {
            Exit::Code(code) => format!("exit code {code}"),
            Exit::Signal(signal) => format!("signal {signal}"),
            Exit::Unknown => "unknown cause".into(),
        };
        (
            format!(
                "{} Minecraft crashed after {played} ({how}). The errors are below.",
                ui::CROSS
            ),
            t.error,
        )
    } else {
        (format!("Minecraft closed after {played}."), t.text)
    };
    cv.text(area.x, area.y, &message, area.width, Style::new().fg(fg));
    let again = widgets::button(
        cv,
        area.x,
        area.y + 2,
        "Play again",
        Some("↵"),
        ButtonKind::Primary,
        Target::Key(KeyCode::Enter),
    );
    let copy = Target::Play(PlayTarget::Copy);
    widgets::button(
        cv,
        again.right() + 2,
        area.y + 2,
        "Copy log",
        Some("c"),
        ButtonKind::Secondary,
        copy,
    );
    area.y + 4
}

/// Why the launch didn't start, and the ways forward.
fn failed(cv: &mut Canvas<'_>, area: Rect, failure: &Failure, offline: bool) {
    let t = *cv.theme();
    let (title, message) = match failure {
        Failure::SignIn(message) => ("Your sign-in needs renewing", message),
        Failure::Launch(message) => ("Couldn't launch Minecraft", message),
    };
    cv.text(
        area.x,
        area.y,
        &format!("{} {title}", ui::CROSS),
        area.width,
        Style::new().fg(t.error).bold(),
    );
    let mut y = area.y + 2;
    for line in text::wrap(message, usize::from(area.width.min(80))) {
        if y >= area.bottom() {
            return;
        }
        cv.text(area.x, y, &line, area.width, Style::new().fg(t.text));
        y += 1;
    }
    y += 1;
    let (label, key, target) = match failure {
        Failure::SignIn(_) => ("Sign in", "↵", PlayTarget::SignIn),
        Failure::Launch(_) => ("Try again", "r", PlayTarget::Retry),
    };
    let first = widgets::button(
        cv,
        area.x,
        y,
        label,
        Some(key),
        ButtonKind::Primary,
        Target::Play(target),
    );
    if !offline {
        let target = Target::Play(PlayTarget::Offline);
        widgets::button(
            cv,
            first.right() + 2,
            y,
            "Play offline",
            Some("o"),
            ButtonKind::Secondary,
            target,
        );
    }
}
