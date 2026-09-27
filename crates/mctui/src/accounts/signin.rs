//! Signing in with Microsoft: a code to enter on Microsoft's site, a wait
//! while the player does, then Minecraft.
//!
//! The whole sign-in (start, poll, finish) runs as one task, numbered so a
//! replaced attempt's late messages are ignored. Leaving the screen drops
//! the task, which cancels it (A-2). Once signed in, the screen closes and
//! opens whatever needed the sign-in, such as the launch that asked (A-8).

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jiff::Timestamp;
use mctui_core::auth::{self, DeviceCode, XboxRefusal};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::{Change, Msg, Step};
use crate::app::{Action, Route};
use crate::data::Data;
use crate::msg;
use crate::overlay::HelpGroup;
use crate::screen::{Ctx, Handled, Screen};
use crate::services::Services;
use crate::task::{Task, Tasks};
use crate::ui::widgets::{self, ButtonKind, Tone};
use crate::ui::{self, Canvas, Hint, Target, text};

const WIDTH: u16 = 72;

#[derive(Debug)]
pub struct SignIn {
    /// Where to go once signed in: the launch that needed it.
    then: Option<Route>,
    state: State,
    /// Numbers attempts, so messages from a replaced one are ignored.
    attempt: u64,
    /// The attempt in flight. Dropping it (or the screen) cancels it.
    task: Option<Task>,
    /// When the state began (the spinner counts from it), and the last tick.
    since: Instant,
    now: Instant,
    body: Rect,
}

#[derive(Debug)]
enum State {
    Contacting,
    /// The code is on screen (and on the clipboard).
    Code {
        code: DeviceCode,
        /// When the code stops working.
        expires: Instant,
    },
    Finishing,
    Failed(auth::Error),
}

impl SignIn {
    /// Starts signing in at once, reporting through `tasks`.
    pub fn new(then: Option<Route>, data: &Data, services: &Services, tasks: &Tasks) -> Self {
        let now = Instant::now();
        let mut screen = Self {
            then,
            state: State::Contacting,
            attempt: 0,
            task: None,
            since: now,
            now,
            body: Rect::default(),
        };
        screen.start(data, services, tasks, now);
        screen
    }

    fn start(&mut self, data: &Data, services: &Services, tasks: &Tasks, now: Instant) {
        self.attempt += 1;
        let attempt = self.attempt;
        let auth = services.auth.clone();
        let client_id = data.config.msa_client_id.clone();
        self.task = Some(tasks.spawn(move |mail| async move {
            let send = |step| mail.send(msg::Msg::Accounts(Msg::SignIn { attempt, step }));
            let result = async {
                let code = auth.start(&client_id).await?;
                send(Step::Code(code.clone()));
                let tokens = auth.poll(&code).await?;
                send(Step::SigningIn);
                auth.finish(tokens).await
            }
            .await;
            send(Step::Done(result));
        }));
        self.set_state(State::Contacting, now);
    }

    fn set_state(&mut self, state: State, now: Instant) {
        self.state = state;
        self.since = now;
        self.now = now;
    }

    fn working(&self) -> bool {
        !matches!(self.state, State::Failed(_))
    }

    /// The instance the player was about to play, if that's why we're here.
    fn playing<'d>(&self, data: &'d Data) -> Option<&'d str> {
        match &self.then {
            Some(Route::Launch { id, .. }) => data.instance(id).map(|i| i.name.as_str()),
            _ => None,
        }
    }

    /// The same launch, offline: the way out for a player who can't sign in.
    fn offline(&self) -> Option<Route> {
        match &self.then {
            Some(Route::Launch { id, .. }) => Some(Route::Launch {
                id: id.clone(),
                offline: true,
            }),
            _ => None,
        }
    }

    fn render_code(&self, cv: &mut Canvas<'_>, area: Rect, mut y: u16) {
        let State::Code { code, expires } = &self.state else {
            return;
        };
        prompt(cv, area, y, &code.verification_uri);
        y += 2;
        code_chip(cv, area, y, &code.user_code);
        y += 2;
        self.buttons(cv, area, y, &[("Open browser", 'b'), ("Copy code", 'c')]);
        y += 2;
        let left = time_left(expires.saturating_duration_since(self.now));
        self.spinner(cv, area, y, "Waiting for you to sign in…", Some(&left));
    }

    fn render_failed(&self, cv: &mut Canvas<'_>, area: Rect, mut y: u16, error: &auth::Error) {
        let t = *cv.theme();
        let (what, detail) = explain(error);
        let title = format!("{}  Couldn't sign in", ui::CROSS);
        let bold = Style::new().fg(t.error).bold();
        cv.text(area.x, y, &title, area.width, bold);
        y += 2;
        let lines = text::wrap(&what, usize::from(area.width))
            .into_iter()
            .map(|line| (line, t.text));
        let details = detail
            .iter()
            .flat_map(|d| text::wrap(d, usize::from(area.width)))
            .map(|line| (line, t.muted));
        for (line, fg) in lines.chain(details) {
            cv.text(area.x, y, &line, area.width, Style::new().fg(fg));
            y += 1;
        }
        self.buttons(cv, area, y + 1, &[("Try again", 'r')]);
    }

    /// A row of buttons, the first the primary one, then "Play offline" when
    /// a launch is waiting.
    fn buttons(&self, cv: &mut Canvas<'_>, area: Rect, y: u16, buttons: &[(&str, char)]) {
        let offline = self.offline().map(|_| ("Play offline", 'o'));
        let mut x = area.x;
        for (i, (label, key)) in buttons.iter().copied().chain(offline).enumerate() {
            let kind = if i == 0 {
                ButtonKind::Primary
            } else {
                ButtonKind::Secondary
            };
            let target = Target::Key(KeyCode::Char(key));
            let keycap = key.to_string();
            x = widgets::button(cv, x, y, label, Some(&keycap), kind, target).right() + 2;
        }
    }

    /// `⠋  Waiting for you to sign in…   14 min left`.
    fn spinner(&self, cv: &mut Canvas<'_>, area: Rect, y: u16, what: &str, note: Option<&str>) {
        let t = *cv.theme();
        let mut spans = vec![
            Span::styled(
                widgets::spinner(self.since, self.now),
                Style::new().fg(t.accent),
            ),
            Span::styled(format!("  {what}"), Style::new().fg(t.subtle)),
        ];
        if let Some(note) = note {
            spans.push(Span::styled(format!("   {note}"), Style::new().fg(t.muted)));
        }
        let line = text::fit(Line::from(spans), usize::from(area.width));
        cv.line(area.x, y, &line, area.width);
    }
}

/// "Go to microsoft.com/link and enter this code:", the site a link.
fn prompt(cv: &mut Canvas<'_>, area: Rect, y: u16, url: &str) {
    const GO_TO: &str = "Go to ";
    let t = *cv.theme();
    let site = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    let browser = Target::Key(KeyCode::Char('b'));
    let link = if cv.hovered(&browser) {
        Style::new().fg(t.accent).bold().underlined()
    } else {
        Style::new().fg(t.accent).bold()
    };
    let plain = Style::new().fg(t.text);
    let prompt = Line::from(vec![
        Span::styled(GO_TO, plain),
        Span::styled(site, link),
        Span::styled(" and enter this code:", plain),
    ]);
    let end = cv.line(
        area.x,
        y,
        &text::fit(prompt, usize::from(area.width)),
        area.width,
    );
    let x = area.x + text::width(GO_TO) as u16;
    let w = (text::width(site) as u16).min(end.saturating_sub(x));
    cv.click(Rect::new(x, y, w, 1), browser);
}

/// The code, big and clear: spaced out on a tinted chip that copies it.
fn code_chip(cv: &mut Canvas<'_>, area: Rect, y: u16, code: &str) {
    let t = *cv.theme();
    let spaced: Vec<String> = code.chars().map(String::from).collect();
    let chip = Line::styled(
        format!("  {}  ", spaced.join(" ")),
        Style::new().fg(t.strong).bold(),
    );
    let copy = Target::Key(KeyCode::Char('c'));
    let chip = widgets::chip(cv, area.x, y, &chip, Tone::Selected, Some(copy), area.width);
    let x = chip.right() + 3;
    let room = area.right().saturating_sub(x);
    let copied = format!("{} Copied", ui::CHECK);
    cv.text(x, y, &copied, room, Style::new().fg(t.success));
}

/// `14 min left`, rounding up so it never says 0 while there's time.
fn time_left(left: Duration) -> String {
    let secs = left.as_secs();
    if secs >= 60 {
        format!("{} min left", secs.div_ceil(60))
    } else {
        format!("{secs} s left")
    }
}

/// What went wrong, for the player, and what to do about it.
fn explain(error: &auth::Error) -> (String, Option<String>) {
    use auth::Error as E;
    let again = || Some("Press r to try again.".to_owned());
    match error {
        E::Expired => (
            "The code expired before you finished signing in.".into(),
            Some("Press r for a new one.".into()),
        ),
        E::Denied => ("Sign-in was declined on Microsoft's page.".into(), again()),
        E::NoMinecraft => (
            "This Microsoft account doesn't own Minecraft Java Edition.".into(),
            Some(
                "With Game Pass, open the official launcher once to set up a profile, \
                 then press r."
                    .into(),
            ),
        ),
        E::Xbox(refusal) => {
            let detail = match refusal {
                XboxRefusal::Region => None,
                _ => Some("Then press r to try again.".to_owned()),
            };
            (sentence(&refusal.to_string()), detail)
        }
        E::Network { service, .. } => (
            format!("Couldn't reach {service} — check your connection, then press r."),
            None,
        ),
        E::Service {
            service,
            status,
            reason,
        } => (
            format!("{service} couldn't sign you in right now — press r to try again."),
            Some(format!("It said: {status}, {reason}.")),
        ),
        E::NeedsSignIn | E::SessionExpired => {
            ("Minecraft didn't accept the sign-in.".into(), again())
        }
        E::Renewed { error, .. } => explain(error),
    }
}

/// `text` as a sentence: a capital letter first, a full stop last.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().collect();
    out.push_str(chars.as_str());
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

impl Screen for SignIn {
    fn crumbs(&self, data: &Data) -> Vec<String> {
        match self.playing(data) {
            Some(name) => vec![name.to_owned(), "Sign in".into()],
            None => vec!["Accounts".into(), "Sign in".into()],
        }
    }

    fn hints(&self, _data: &Data) -> Vec<Hint> {
        let key = Hint::key;
        let mut hints = match &self.state {
            State::Code { .. } => vec![
                key(KeyCode::Char('b'), "open browser"),
                key(KeyCode::Char('c'), "copy code"),
            ],
            State::Failed(_) => vec![key(KeyCode::Char('r'), "try again")],
            State::Contacting | State::Finishing => Vec::new(),
        };
        if self.offline().is_some() {
            hints.push(key(KeyCode::Char('o'), "play offline"));
        }
        let esc = if self.working() { "cancel" } else { "back" };
        hints.push(key(KeyCode::Esc, esc));
        hints
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        let sign_in = HelpGroup {
            title: "Sign in",
            entries: if self.offline().is_some() {
                &[
                    ("b ↵", "open Microsoft's page"),
                    ("c", "copy the code"),
                    ("r", "try again"),
                    ("o", "play offline instead"),
                ]
            } else {
                &[
                    ("b ↵", "open Microsoft's page"),
                    ("c", "copy the code"),
                    ("r", "try again"),
                ]
            },
        };
        let general = HelpGroup {
            title: "General",
            entries: &[("esc", "cancel"), ("?", "help")],
        };
        ("Sign in", vec![sign_in, general])
    }

    fn set_body(&mut self, body: Rect) {
        self.body = body;
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Handled::Ignored;
        }
        match (key.code, &self.state) {
            // Closing the screen drops the task, which cancels the sign-in.
            (KeyCode::Esc, _) => Handled::Action(Action::Close),
            (KeyCode::Char('c'), State::Code { code, .. }) => {
                Handled::Action(Action::Copy(code.user_code.clone()))
            }
            (KeyCode::Char('b') | KeyCode::Enter, State::Code { code, .. }) => {
                Handled::Action(Action::OpenUrl(code.verification_uri.clone()))
            }
            (KeyCode::Char('r') | KeyCode::Enter, State::Failed(_)) => {
                self.start(cx.data, cx.services, cx.tasks, cx.now);
                Handled::Consumed
            }
            (KeyCode::Char('o'), _) => self.offline().map_or(Handled::Ignored, |offline| {
                Handled::Action(Action::Many(vec![Action::Close, Action::Open(offline)]))
            }),
            _ => Handled::Ignored,
        }
    }

    fn handle_msg(&mut self, msg: msg::Msg, cx: &mut Ctx<'_>) -> Handled {
        let msg::Msg::Accounts(Msg::SignIn { attempt, step }) = msg else {
            return Handled::Ignored;
        };
        if attempt != self.attempt {
            return Handled::Ignored;
        }
        match step {
            Step::Code(code) => {
                // Count down on the monotonic clock from what's left now.
                let left = Duration::try_from(Timestamp::now().duration_until(code.expires_at))
                    .unwrap_or_default();
                let expires = cx.now.checked_add(left).unwrap_or(cx.now);
                // Copy it straight away: most players paste it.
                let copy = Action::Copy(code.user_code.clone());
                self.set_state(State::Code { code, expires }, cx.now);
                Handled::Action(copy)
            }
            Step::SigningIn => {
                self.set_state(State::Finishing, cx.now);
                Handled::Consumed
            }
            Step::Done(Ok(account)) => {
                self.task = None;
                let mut actions = vec![
                    Action::Account(Change::SignedIn(Box::new(account))),
                    Action::Close,
                ];
                actions.extend(self.then.take().map(Action::Open));
                Handled::Action(Action::Many(actions))
            }
            Step::Done(Err(error)) => {
                self.task = None;
                self.set_state(State::Failed(error), cx.now);
                Handled::Consumed
            }
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.working()
            .then(|| widgets::next_spin(self.since, self.now))
    }

    fn tick(&mut self, now: Instant) -> bool {
        self.now = now;
        self.working()
    }

    fn render(&self, cv: &mut Canvas<'_>, data: &Data) {
        let t = *cv.theme();
        let area = Rect {
            x: self.body.x + 2,
            width: self.body.width.saturating_sub(4).min(WIDTH),
            ..self.body
        };
        let mut y = area.y;
        if let Some(name) = self.playing(data) {
            let line = format!("Sign in with Microsoft to play {name} online.");
            cv.text(area.x, y, &line, area.width, Style::new().fg(t.dim));
            y += 2;
        }
        match &self.state {
            State::Contacting => self.spinner(cv, area, y, "Contacting Microsoft…", None),
            State::Code { .. } => self.render_code(cv, area, y),
            State::Finishing => self.spinner(cv, area, y, "Signing in to Minecraft…", None),
            State::Failed(error) => self.render_failed(cv, area, y, error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_left_rounds_up() {
        assert_eq!(time_left(Duration::from_secs(900)), "15 min left");
        assert_eq!(time_left(Duration::from_secs(61)), "2 min left");
        assert_eq!(time_left(Duration::from_secs(60)), "1 min left");
        assert_eq!(time_left(Duration::from_secs(59)), "59 s left");
    }

    #[test]
    fn refusals_read_as_sentences() {
        assert_eq!(sentence("xbox said no"), "Xbox said no.");
        assert_eq!(sentence("done."), "Done.");
        assert_eq!(sentence(""), "");
    }
}
