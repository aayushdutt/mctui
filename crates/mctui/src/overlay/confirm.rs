//! Yes/no confirmation. Destructive ones open on the safe choice.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::{Outcome, Target};
use crate::app::Action;
use crate::theme;
use crate::ui::{self, Canvas, Hint, text};

#[derive(Debug)]
pub struct Confirm {
    pub title: String,
    pub message: String,
    /// A caution line under the message, e.g. "This cannot be undone."
    pub warning: Option<String>,
    pub yes: &'static str,
    pub no: &'static str,
    /// Destructive: red accent.
    pub danger: bool,
    pub on_yes: Action,
    focus_yes: bool,
}

const WIDTH: u16 = 56;

impl Confirm {
    /// A confirmation that opens on the safe choice.
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        yes: &'static str,
        on_yes: Action,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            warning: None,
            yes,
            no: "Cancel",
            danger: false,
            on_yes,
            focus_yes: false,
        }
    }

    pub fn danger(mut self, warning: impl Into<String>) -> Self {
        self.danger = true;
        self.warning = Some(warning.into());
        self
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Char('h' | 'j' | 'k' | 'l') => {
                self.focus_yes = !self.focus_yes;
                Outcome::Stay
            }
            KeyCode::Char('y' | 'Y') => self.answer(true),
            KeyCode::Enter => self.answer(self.focus_yes),
            KeyCode::Char('n' | 'N' | 'q') | KeyCode::Esc => Outcome::Close,
            _ => Outcome::Stay,
        }
    }

    pub fn answer(&self, yes: bool) -> Outcome {
        if yes {
            Outcome::Run(self.on_yes.clone())
        } else {
            Outcome::Close
        }
    }

    pub fn render(&self, cv: &mut Canvas<'_>, screen: Rect) {
        let t = *cv.theme();
        let accent = if self.danger { t.error } else { t.accent };
        let width = ui::card_width(screen.width, WIDTH);
        let inner = usize::from(width.saturating_sub(4));
        let mut message = text::wrap(&self.message, inner);
        let mut warning = self
            .warning
            .as_ref()
            .map(|w| text::wrap(&format!("{} {w}", ui::WARN), inner));
        let mut hints = true;

        // Message, blank, [warning, blank], buttons, [blank, hints]. On a short
        // screen the extras go first, so the choice is always on screen.
        let room = usize::from(screen.height.saturating_sub(ui::CARD_CHROME));
        let height = |message: &[String], warning: &Option<Vec<String>>, hints: bool| {
            message.len()
                + 2
                + warning.as_ref().map_or(0, |w| w.len() + 1)
                + if hints { 2 } else { 0 }
        };
        if height(&message, &warning, hints) > room {
            hints = false;
        }
        if height(&message, &warning, hints) > room {
            warning = None;
        }
        message.truncate(room.saturating_sub(2).max(1));
        let body_h = height(&message, &warning, hints) as u16;

        let rect = ui::card_rect(screen, width, body_h, None);
        super::card(cv, rect, &self.title, accent, |cv, body| {
            let mut y = body.y;
            for line in &message {
                cv.text(body.x, y, line, body.width, Style::new().fg(t.text));
                y += 1;
            }
            y += 1;
            if let Some(warning) = &warning {
                for line in warning {
                    cv.text(body.x, y, line, body.width, Style::new().fg(accent));
                    y += 1;
                }
                y += 1;
            }
            let yes = button(cv, body.x, y, self.yes, self.focus_yes, Target::Yes, accent);
            // Red only ever fills the destructive choice; the safe one uses the accent.
            button(
                cv,
                yes.right() + 2,
                y,
                self.no,
                !self.focus_yes,
                Target::No,
                t.accent,
            );
            if hints {
                let hints = [
                    Hint::show("←→", "choose"),
                    Hint::key(KeyCode::Enter, "confirm"),
                    Hint::key(KeyCode::Esc, "cancel"),
                ];
                cv.hints(Rect::new(body.x, y + 2, body.width, 1), &hints);
            }
        });
    }
}

/// The focused choice is a filled single-row button; the other sits on a
/// quiet surface, brightening under the pointer. Returns its area.
fn button(
    cv: &mut Canvas<'_>,
    x: u16,
    y: u16,
    label: &str,
    focused: bool,
    target: Target,
    accent: Color,
) -> Rect {
    let t = *cv.theme();
    let target = ui::Target::Overlay(target);
    let (fg, bg) = match (focused, cv.hovered(&target)) {
        (true, _) => (theme::on_color(accent), accent),
        (false, true) => (t.strong, t.surface_hi),
        (false, false) => (t.text, t.surface),
    };
    let style = if focused {
        Style::new().fg(fg).bold()
    } else {
        Style::new().fg(fg)
    };
    let line = Line::from(Span::styled(format!(" {label} "), style));
    let area = cv.pill(x, y, &line, bg, 30);
    cv.click(area, target);
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog() -> Confirm {
        Confirm::new(
            "Delete instance?",
            "Delete \"Survival\"?",
            "Delete",
            Action::Quit,
        )
        .danger("This cannot be undone.")
    }

    #[test]
    fn opens_on_the_safe_choice() {
        let mut c = dialog();
        assert!(matches!(
            c.handle_key(KeyEvent::from(KeyCode::Enter)),
            Outcome::Close
        ));
    }

    #[test]
    fn toggle_then_enter_confirms() {
        let mut c = dialog();
        c.handle_key(KeyEvent::from(KeyCode::Left));
        assert!(matches!(
            c.handle_key(KeyEvent::from(KeyCode::Enter)),
            Outcome::Run(_)
        ));
    }

    #[test]
    fn y_confirms_and_cancel_keys_close() {
        assert!(matches!(
            dialog().handle_key(KeyEvent::from(KeyCode::Char('y'))),
            Outcome::Run(_)
        ));
        for code in [KeyCode::Char('n'), KeyCode::Char('q'), KeyCode::Esc] {
            assert!(matches!(
                dialog().handle_key(KeyEvent::from(code)),
                Outcome::Close
            ));
        }
        // Anything else is swallowed.
        assert!(matches!(
            dialog().handle_key(KeyEvent::from(KeyCode::Char('d'))),
            Outcome::Stay
        ));
    }
}
