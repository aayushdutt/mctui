//! The `?` key reference for the current screen.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::Outcome;
use crate::ui::{self, Canvas, Hint, text};

/// A titled cluster of keys, e.g. "Navigate".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpGroup {
    pub title: &'static str,
    /// `(keys, what they do)`.
    pub entries: &'static [(&'static str, &'static str)],
}

/// One row of a help column.
#[derive(Debug, Clone, Copy)]
enum Row {
    Title(&'static str),
    Entry(&'static str, &'static str),
    Blank,
}

#[derive(Debug)]
pub struct Help {
    title: String,
    groups: Vec<HelpGroup>,
    /// First row shown, when the reference is taller than the screen.
    scroll: usize,
    /// The furthest `scroll` can go, as of the last frame.
    max_scroll: Cell<usize>,
}

impl Help {
    pub fn new(screen: &str, groups: Vec<HelpGroup>) -> Self {
        Self {
            title: format!("{screen} · keys"),
            groups,
            scroll: 0,
            max_scroll: Cell::new(0),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?' | 'q') => return Outcome::Close,
            KeyCode::Down | KeyCode::Char('j') => self.scroll_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_by(-1),
            KeyCode::PageDown => self.scroll_by(10),
            KeyCode::PageUp => self.scroll_by(-10),
            _ => false,
        };
        Outcome::Stay
    }

    pub fn scroll_by(&mut self, delta: isize) -> bool {
        let next = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.max_scroll.get());
        std::mem::replace(&mut self.scroll, next) != next
    }

    pub fn render(&self, cv: &mut Canvas<'_>, screen: Rect) {
        let t = *cv.theme();
        let entries = || self.groups.iter().flat_map(|g| g.entries);
        let key_w = entries().map(|(k, _)| text::width(k)).max().unwrap_or(0) as u16;
        let desc_w = entries().map(|(_, d)| text::width(d)).max().unwrap_or(0) as u16;
        let col_w = key_w + 2 + desc_w;

        // One column if it fits; otherwise two balanced ones if the screen is
        // wide enough; otherwise one column that scrolls.
        let one = rows(&self.groups);
        let room = screen.height.saturating_sub(ui::CARD_CHROME + 2);
        let columns: Vec<Vec<Row>> =
            if one.len() as u16 > room && self.groups.len() > 1 && 2 * col_w + 12 <= screen.width {
                let split = (1..self.groups.len())
                    .min_by_key(|&i| {
                        let (a, b) = self.groups.split_at(i);
                        rows(a).len().max(rows(b).len())
                    })
                    .unwrap_or(1);
                let (a, b) = self.groups.split_at(split);
                vec![rows(a), rows(b)]
            } else {
                vec![one]
            };
        let tallest = columns.iter().map(Vec::len).max().unwrap_or(0);
        let visible = tallest.min(usize::from(room.max(1)));
        self.max_scroll.set(tallest - visible);
        let scroll = self.scroll.min(tallest - visible);

        let body_w = col_w * columns.len() as u16 + 4 * (columns.len() as u16 - 1);
        let width = (body_w + 4)
            .max(40)
            .min(screen.width.saturating_sub(4).max(24));
        let rect = ui::card_rect(screen, width, visible as u16 + 2, None);
        super::card(cv, rect, &self.title, t.accent, |cv, body| {
            for (c, column) in columns.iter().enumerate() {
                let x = body.x + c as u16 * (col_w + 4);
                for (i, row) in column.iter().skip(scroll).take(visible).enumerate() {
                    let y = body.y + i as u16;
                    match row {
                        Row::Title(title) => {
                            cv.text(x, y, title, col_w, Style::new().fg(t.title).bold());
                        }
                        Row::Entry(keys, desc) => {
                            cv.text(x, y, keys, key_w, Style::new().fg(t.accent).bold());
                            cv.text(x + key_w + 2, y, desc, desc_w, Style::new().fg(t.dim));
                        }
                        Row::Blank => {}
                    }
                }
            }
            let mut hints = vec![Hint::key(KeyCode::Esc, "close")];
            if tallest > visible {
                hints.insert(0, Hint::show("↑↓", "scroll"));
            }
            let hint_row = Rect::new(body.x, body.bottom().saturating_sub(1), body.width, 1);
            cv.hints(hint_row, &hints);
        });
    }
}

/// The groups as rows: title, entries, and a blank line between groups.
fn rows(groups: &[HelpGroup]) -> Vec<Row> {
    let mut out = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            out.push(Row::Blank);
        }
        out.push(Row::Title(group.title));
        out.extend(group.entries.iter().map(|(k, d)| Row::Entry(k, d)));
    }
    out
}
