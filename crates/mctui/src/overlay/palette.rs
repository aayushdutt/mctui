//! The `:` command palette: type to fuzzy-filter commands and jump targets.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::{Outcome, Target};
use crate::app::Action;
use crate::fuzzy::{self, Match};
use crate::ui::{self, Canvas, Hint, input::TextInput, text};

#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub label: String,
    /// The screen shortcut that does the same thing, shown so the palette
    /// teaches the keys.
    pub key: Option<&'static str>,
    /// Jump rows (go to an instance) get a quiet dot.
    pub jump: bool,
    pub action: Action,
}

#[derive(Debug)]
pub struct Palette {
    input: TextInput,
    items: Vec<PaletteItem>,
    matches: Vec<Match>,
    cursor: usize,
}

const MAX_ROWS: u16 = 8;
const WIDTH: u16 = 56;

impl Palette {
    pub fn new(items: Vec<PaletteItem>) -> Self {
        let mut p = Self {
            input: TextInput::default(),
            items,
            matches: Vec::new(),
            cursor: 0,
        };
        p.refilter();
        p
    }

    fn refilter(&mut self) {
        self.matches = fuzzy::rank(
            self.input.as_str(),
            self.items.iter().map(|i| i.label.as_str()),
        );
        // A new query re-ranks: highlight the best match again.
        self.cursor = 0;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        match key.code {
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Enter => return self.run(),
            KeyCode::Up => self.step(false),
            KeyCode::Char('p') if ctrl => self.step(false),
            KeyCode::Down => self.step(true),
            KeyCode::Char('n') if ctrl => self.step(true),
            _ => {
                if self.input.handle_key(key) == Some(true) {
                    self.refilter();
                }
                true
            }
        };
        Outcome::Stay
    }

    pub fn paste(&mut self, text: &str) {
        if self.input.insert(text) {
            self.refilter();
        }
    }

    pub fn step(&mut self, down: bool) -> bool {
        let next = if down {
            (self.cursor + 1).min(self.matches.len().saturating_sub(1))
        } else {
            self.cursor.saturating_sub(1)
        };
        std::mem::replace(&mut self.cursor, next) != next
    }

    pub fn select(&mut self, row: usize) -> bool {
        row < self.matches.len() && std::mem::replace(&mut self.cursor, row) != row
    }

    pub fn run(&self) -> Outcome {
        match self.matches.get(self.cursor) {
            Some(m) => Outcome::Run(self.items[m.index].action.clone()),
            None => Outcome::Close,
        }
    }

    pub fn render(&self, cv: &mut Canvas<'_>, screen: Rect) {
        let t = *cv.theme();
        let rows = MAX_ROWS.min(screen.height.saturating_sub(12)).max(1);
        let list_h = if self.matches.is_empty() {
            1
        } else {
            rows.min(self.matches.len() as u16)
        };
        let width = ui::card_width(screen.width, WIDTH);
        // Input, blank, list, blank, hints.
        let rect = ui::card_rect(screen, width, list_h + 4, None);
        super::card(cv, rect, "Command palette", t.accent, |cv, body| {
            // The query sits in a filled field, so it reads as somewhere to type.
            let input = Rect { height: 1, ..body };
            cv.band(input, t.surface);
            let prompt = Style::new().fg(t.accent).bold();
            cv.text(input.x + 1, input.y, ":", 1, prompt);
            let field = Rect {
                x: input.x + 3,
                width: input.width.saturating_sub(4),
                ..input
            };
            self.input.render(
                cv,
                field,
                Style::new().fg(t.strong),
                "Jump to an instance or run a command…",
                true,
            );

            let list = Rect::new(body.x, body.y + 2, body.width, list_h);
            if self.matches.is_empty() {
                cv.text(
                    list.x + 2,
                    list.y,
                    "No matches",
                    list.width,
                    Style::new().fg(t.muted),
                );
            }
            let tags = self.matches.iter().any(|m| self.items[m.index].jump);
            // Keep the highlighted row in view.
            let start = self.cursor.saturating_sub(usize::from(rows) - 1);
            for (row, m) in self
                .matches
                .iter()
                .enumerate()
                .skip(start)
                .take(usize::from(list_h))
            {
                let y = list.y + (row - start) as u16;
                let area = Rect::new(list.x, y, list.width, 1);
                render_row(cv, area, &self.items[m.index], m, row == self.cursor, tags);
                cv.click(area, ui::Target::Overlay(Target::Row(row)));
            }

            let hints = [
                Hint::show("↑↓", "move"),
                Hint::key(KeyCode::Enter, "run"),
                Hint::key(KeyCode::Esc, "close"),
            ];
            cv.hints(Rect::new(body.x, list.bottom() + 1, body.width, 1), &hints);
        });
    }
}

/// `tags`: reserve the jump-dot column (some shown row is a jump).
fn render_row(
    cv: &mut Canvas<'_>,
    area: Rect,
    item: &PaletteItem,
    m: &Match,
    selected: bool,
    tags: bool,
) {
    let t = *cv.theme();
    let label_style = if selected {
        Style::new().fg(t.strong).bold()
    } else {
        Style::new().fg(t.text)
    };
    if selected {
        cv.band(area, t.selection);
    }
    let mut spans = vec![Span::raw(" ")];
    if tags {
        let tag = if item.jump { "● " } else { "  " };
        spans.push(Span::styled(tag, Style::new().fg(t.muted)));
    }
    spans.extend(text::highlight(
        &item.label,
        &m.positions,
        label_style,
        label_style.fg(t.accent),
    ));

    let key_w = item.key.map_or(1, |k| text::width(k) as u16 + 3);
    if let Some(key) = item.key {
        let inner = Rect {
            width: area.width.saturating_sub(1),
            ..area
        };
        cv.line_right(inner, area.y, &Line::styled(key, Style::new().fg(t.muted)));
    }
    let room = area.width.saturating_sub(key_w);
    let line = text::fit(Line::from(spans), usize::from(room));
    cv.line(area.x, area.y, &line, room);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<PaletteItem> {
        ["Play", "Mods", "Settings"]
            .into_iter()
            .map(|label| PaletteItem {
                label: label.to_owned(),
                key: None,
                jump: false,
                action: Action::Quit,
            })
            .collect()
    }

    #[test]
    fn typing_refilters_and_resets_the_highlight() {
        let mut p = Palette::new(items());
        p.step(true);
        assert_eq!(p.cursor, 1);
        p.handle_key(KeyEvent::from(KeyCode::Char('s')));
        assert_eq!(p.cursor, 0);
        assert!(
            p.matches
                .iter()
                .all(|m| p.items[m.index].label.to_lowercase().contains('s'))
        );
    }
}
