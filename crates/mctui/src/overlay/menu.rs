//! A small list of actions with hotkeys: the `x` actions card, and the
//! right-click context menu (the same card, opened at the pointer).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;

use super::{Outcome, Target};
use crate::app::Action;
use crate::ui::{self, Canvas, Hint, text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: &'static str,
    /// Pressing this runs the entry; shown flush right.
    pub key: KeyCode,
    /// Destructive entries read in the error colour.
    pub danger: bool,
    pub action: Action,
}

#[derive(Debug)]
pub struct Menu {
    title: String,
    items: Vec<MenuItem>,
    cursor: usize,
    /// Where a right-click opened it; centred when `None`.
    anchor: Option<Position>,
}

impl Menu {
    pub fn new(title: impl Into<String>, items: Vec<MenuItem>, anchor: Option<Position>) -> Self {
        Self {
            title: title.into(),
            items,
            cursor: 0,
            anchor,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Outcome::Close,
            KeyCode::Enter => self.run(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.step(false);
                Outcome::Stay
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.step(true);
                Outcome::Stay
            }
            code => match self.items.iter().position(|i| i.key == code) {
                Some(i) => {
                    self.cursor = i;
                    self.run()
                }
                None => Outcome::Stay,
            },
        }
    }

    pub fn step(&mut self, down: bool) -> bool {
        let next = if down {
            (self.cursor + 1).min(self.items.len().saturating_sub(1))
        } else {
            self.cursor.saturating_sub(1)
        };
        std::mem::replace(&mut self.cursor, next) != next
    }

    pub fn select(&mut self, row: usize) -> bool {
        row < self.items.len() && std::mem::replace(&mut self.cursor, row) != row
    }

    pub fn run(&self) -> Outcome {
        self.items
            .get(self.cursor)
            .map_or(Outcome::Close, |i| Outcome::Run(i.action.clone()))
    }

    pub fn render(&self, cv: &mut Canvas<'_>, screen: Rect) {
        let t = *cv.theme();
        // A context menu stays compact; the keyboard card teaches its keys.
        let (width, hints) = match self.anchor {
            Some(_) => (ui::card_width(screen.width, 34), false),
            None => (ui::card_width(screen.width, 44), true),
        };
        let rows = self.items.len() as u16;
        let body_h = if hints { rows + 2 } else { rows };
        let rect = ui::card_rect(screen, width, body_h, self.anchor);
        super::card(cv, rect, &self.title, t.accent, |cv, body| {
            for (i, item) in self.items.iter().enumerate() {
                let area = Rect::new(body.x, body.y + i as u16, body.width, 1);
                if area.y >= body.bottom() {
                    break;
                }
                let selected = i == self.cursor;
                let fg = match (item.danger, selected) {
                    (true, _) => t.error,
                    (false, true) => t.strong,
                    (false, false) => t.text,
                };
                let label_style = if selected {
                    Style::new().fg(fg).bold()
                } else {
                    Style::new().fg(fg)
                };
                if selected {
                    cv.band(area, t.selection);
                }
                let inner = Rect {
                    x: area.x + 1,
                    width: area.width.saturating_sub(2),
                    ..area
                };
                let key = Line::styled(ui::key_name(item.key), Style::new().fg(t.muted));
                let right = cv.line_right(inner, area.y, &key);
                let room = right.x.saturating_sub(inner.x + 1);
                let label = text::truncate(item.label, usize::from(room));
                cv.text(inner.x, area.y, &label, room, label_style);
                cv.click(area, ui::Target::Overlay(Target::Row(i)));
            }
            if hints {
                let row = Rect::new(body.x, body.y + rows + 1, body.width, 1);
                cv.hints(
                    row,
                    &[
                        Hint::show("↑↓", "move"),
                        Hint::key(KeyCode::Enter, "run"),
                        Hint::key(KeyCode::Esc, "close"),
                    ],
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Menu {
        let item = |label, key| MenuItem {
            label,
            key,
            danger: false,
            action: Action::Quit,
        };
        Menu::new(
            "Survival",
            vec![
                item("Edit", KeyCode::Char('e')),
                item("Open folder", KeyCode::Char('f')),
            ],
            None,
        )
    }

    #[test]
    fn hotkeys_run_their_entry() {
        let mut m = menu();
        assert!(matches!(
            m.handle_key(KeyEvent::from(KeyCode::Char('f'))),
            Outcome::Run(_)
        ));
        assert_eq!(m.cursor, 1);
        assert!(matches!(
            m.handle_key(KeyEvent::from(KeyCode::Char('z'))),
            Outcome::Stay
        ));
        assert!(matches!(
            m.handle_key(KeyEvent::from(KeyCode::Char('q'))),
            Outcome::Close
        ));
    }

    #[test]
    fn movement_stops_at_the_ends() {
        let mut m = menu();
        assert!(!m.step(false));
        assert!(m.step(true));
        assert!(!m.step(true));
    }
}
