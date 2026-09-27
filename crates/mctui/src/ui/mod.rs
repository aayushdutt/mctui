//! The shared UI vocabulary every screen draws with: one header, one rule, one
//! titled zone, one key-hint bar, one floating card. Screens compose these
//! instead of hand-rolling chrome, which keeps the app consistent.
//!
//! Hard rule (the user's terminal adds line spacing): never draw a vertical
//! stroke spanning rows and never fill more than one row with a background
//! colour. Structure comes from whitespace; emphasis from single-row tints —
//! the selection band, chips, keycaps and buttons.

pub mod hits;
pub mod input;
pub mod text;

use std::borrow::Cow;

use crossterm::event::KeyCode;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub use hits::{Hits, Target};

use crate::theme::Theme;

pub const DOT: &str = "●";
pub const CHECK: &str = "✓";
pub const WARN: &str = "▲";

/// Read-only state a frame is drawn against.
#[derive(Debug, Clone)]
pub struct Cx {
    pub theme: Theme,
    /// What the mouse is over, from the last pointer move.
    pub hover: Option<Target>,
    pub now: Timestamp,
    pub tz: TimeZone,
}

/// A drawing surface for one frame: the buffer plus the hit map and cursor the
/// frame produces.
pub struct Canvas<'a> {
    pub buf: &'a mut Buffer,
    pub cx: &'a Cx,
    pub hits: Hits,
    /// Where to show the terminal cursor (a focused text field), if anywhere.
    pub cursor: Option<Position>,
    /// Nothing is drawn, and nothing is clickable, outside this.
    clip: Rect,
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut Buffer, cx: &'a Cx) -> Self {
        let clip = buf.area;
        Self {
            buf,
            cx,
            hits: Hits::default(),
            cursor: None,
            clip,
        }
    }

    /// Runs `draw` with drawing and clicks confined to `area`, so a region
    /// that runs out of room is cut off instead of spilling into its
    /// neighbours.
    pub fn clipped<R>(&mut self, area: Rect, draw: impl FnOnce(&mut Self) -> R) -> R {
        let outer = self.clip;
        self.clip = outer.intersection(area);
        let result = draw(self);
        self.clip = outer;
        result
    }

    pub fn theme(&self) -> &Theme {
        &self.cx.theme
    }

    pub fn hovered(&self, target: &Target) -> bool {
        self.cx.hover.as_ref() == Some(target)
    }

    pub fn click(&mut self, area: Rect, target: Target) {
        self.hits.push(area.intersection(self.clip), target);
    }

    /// Draws `s` at (x, y), clipped to `max` cells and the clip area.
    /// Returns the x after it.
    pub fn text(&mut self, x: u16, y: u16, s: &str, max: u16, style: Style) -> u16 {
        let max = self.room(x, y, max);
        if max == 0 {
            return x;
        }
        self.buf.set_stringn(x, y, s, usize::from(max), style).0
    }

    /// Draws a styled line at (x, y), clipped to `max` cells and the clip
    /// area. Returns the x after it.
    pub fn line(&mut self, x: u16, y: u16, line: &Line<'_>, max: u16) -> u16 {
        let max = self.room(x, y, max);
        if max == 0 {
            return x;
        }
        self.buf.set_line(x, y, line, max).0
    }

    /// How many of `want` cells from (x, y) are inside the clip area.
    fn room(&self, x: u16, y: u16, want: u16) -> u16 {
        let area = self.clip;
        if !area.contains(Position::new(x, y)) {
            return 0;
        }
        want.min(area.right() - x)
    }

    /// Draws `line` flush right against `area`'s right edge on row `y`.
    pub fn line_right(&mut self, area: Rect, y: u16, line: &Line<'_>) -> Rect {
        let w = (line.width() as u16).min(area.width);
        let x = area.right() - w;
        self.line(x, y, line, w);
        Rect::new(x, y, w, 1)
    }

    /// Tints one row — a selection or hover band.
    pub fn band(&mut self, row: Rect, bg: Color) {
        let row = Rect {
            height: 1.min(row.height),
            ..row
        }
        .intersection(self.clip);
        for pos in row.positions() {
            if let Some(cell) = self.buf.cell_mut(pos) {
                cell.bg = bg;
            }
        }
    }

    /// A one-row pill: `line` padded by a space each side on `bg`. Returns its
    /// area (clipped to `max` cells).
    pub fn pill(&mut self, x: u16, y: u16, line: &Line<'_>, bg: Color, max: u16) -> Rect {
        let w = (line.width() as u16 + 2).min(max);
        let area = Rect::new(x, y, w, 1);
        self.band(area, bg);
        self.line(x + 1, y, line, w.saturating_sub(2));
        area
    }

    /// A horizontal hairline.
    pub fn rule(&mut self, x: u16, y: u16, width: u16) {
        let style = Style::new().fg(self.cx.theme.rule);
        let row = Rect::new(x, y, width, 1).intersection(self.clip);
        for pos in row.positions() {
            if let Some(cell) = self.buf.cell_mut(pos) {
                cell.set_symbol("─").set_style(style);
            }
        }
    }

    /// Resets `area` to blank cells.
    pub fn clear(&mut self, area: Rect) {
        self.buf.set_style(area, Style::reset());
        for pos in area.positions() {
            if let Some(cell) = self.buf.cell_mut(pos) {
                cell.set_symbol(" ");
            }
        }
    }

    /// Repaints everything in `area` in one barely-there tone — the backdrop
    /// behind a floating card, so the screen stays in view but recedes.
    pub fn dim(&mut self, area: Rect) {
        let faint = self.cx.theme.backdrop;
        for pos in area.positions() {
            if let Some(cell) = self.buf.cell_mut(pos) {
                cell.fg = faint;
                cell.bg = Color::Reset;
                cell.modifier = Modifier::empty();
            }
        }
    }

    /// The one header every screen starts with: the wordmark and a breadcrumb
    /// trail on the left, optional context flush right. When both don't fit,
    /// the right side goes and the trail is truncated.
    pub fn header(&mut self, row: Rect, crumbs: &[&str], right: Option<(Line<'_>, Target)>) {
        let t = self.cx.theme;
        let mut spans = vec![Span::styled("mctui", Style::new().fg(t.accent).bold())];
        for (i, crumb) in crumbs.iter().enumerate() {
            spans.push(Span::styled("  ›  ", Style::new().fg(t.faint)));
            let fg = if i + 1 == crumbs.len() {
                t.strong
            } else {
                t.dim
            };
            spans.push(Span::styled(*crumb, Style::new().fg(fg)));
        }
        let left = Line::from(spans);
        if let Some((right, target)) = right
            && left.width() + 2 + right.width() <= usize::from(row.width)
        {
            let hovered = self.hovered(&target);
            let right = if hovered { underline(right) } else { right };
            let area = self.line_right(row, row.y, &right);
            self.click(area, target);
        }
        self.line(
            row.x,
            row.y,
            &text::fit(left, usize::from(row.width)),
            row.width,
        );
    }

    /// A titled region without a box: a bold title (accent when focused, muted
    /// when not) with an optional annotation flush right, then a blank row.
    /// Returns the body area below.
    pub fn zone(
        &mut self,
        area: Rect,
        title: &str,
        right: Option<Line<'_>>,
        focused: bool,
    ) -> Rect {
        if area.height < 2 || title.is_empty() {
            return area;
        }
        let t = self.cx.theme;
        let mut room = area.width.saturating_sub(2);
        if let Some(right) = right {
            let rw = right.width() as u16;
            if rw + 4 < room {
                room -= rw + 2;
                let inset = Rect {
                    width: area.width.saturating_sub(2),
                    ..area
                };
                self.line_right(inset, area.y, &right);
            }
        }
        let fg = if focused { t.accent } else { t.muted };
        let title = text::truncate(title, usize::from(room));
        self.text(area.x + 2, area.y, &title, room, Style::new().fg(fg).bold());
        Rect {
            y: area.y + 2,
            height: area.height - 2,
            ..area
        }
    }

    /// The key-hint bar: each key on a keycap, its action dim beside it; wraps
    /// to fit. Hints with a key are buttons that press it.
    pub fn hints(&mut self, area: Rect, hints: &[Hint]) {
        let t = self.cx.theme;
        for (hint, pos) in hints.iter().zip(hint_layout(hints, area.width)) {
            let (x, y) = (area.x + pos.x, area.y + pos.y);
            if y >= area.bottom() {
                break;
            }
            let target = hint.press.map(Target::Key);
            let hovered = target.as_ref().is_some_and(|t| self.hovered(t));
            let (cap, label_fg) = if hovered {
                (t.surface_hi, t.text)
            } else {
                (t.surface, t.dim)
            };
            let key = Line::styled(hint.keys.as_ref(), Style::new().fg(t.strong).bold());
            let end = self.pill(x, y, &key, cap, area.right() - x).right();
            let end = if hint.label.is_empty() {
                end
            } else {
                let room = area.right().saturating_sub(end + 1);
                self.text(end + 1, y, hint.label, room, Style::new().fg(label_fg))
            };
            if let Some(target) = target {
                self.click(Rect::new(x, y, end - x, 1), target);
            }
        }
    }

    /// A floating card: a cleared rectangle with a bold title over a rule, the
    /// body, and a closing rule — text and horizontal rules only. Returns the
    /// body area (see [`card_rect`] for sizing).
    pub fn card(&mut self, rect: Rect, title: &str, accent: Color) -> Rect {
        self.clear(rect);
        let inner_w = rect.width.saturating_sub(4);
        let x = rect.x + 2;
        let title = text::truncate(title, usize::from(inner_w));
        self.text(
            x,
            rect.y + 1,
            &title,
            inner_w,
            Style::new().fg(accent).bold(),
        );
        self.rule(x, rect.y + 2, inner_w);
        self.rule(x, rect.bottom().saturating_sub(2), inner_w);
        Rect::new(
            x,
            rect.y + 4,
            inner_w,
            rect.height.saturating_sub(CARD_CHROME),
        )
    }
}

/// Rows a card adds around its body: blank, title, rule, blank above; blank,
/// rule, blank below.
pub const CARD_CHROME: u16 = 7;

/// Where a card with a `width`-cell frame and `body_h` rows of body goes on
/// `screen`: centred, or near `anchor` (a right-click) when given. Always kept
/// on screen.
pub fn card_rect(screen: Rect, width: u16, body_h: u16, anchor: Option<Position>) -> Rect {
    let w = width.min(screen.width);
    let h = (body_h + CARD_CHROME).min(screen.height);
    let (x, y) = match anchor {
        Some(p) => (p.x.min(screen.right() - w), p.y.min(screen.bottom() - h)),
        None => (
            screen.x + (screen.width - w) / 2,
            screen.y + (screen.height - h) / 2,
        ),
    };
    Rect::new(x, y, w, h)
}

/// A card width for a screen: `want`, but never wider than the screen allows
/// (8 cells of margin) and never narrower than a readable floor.
pub fn card_width(screen_w: u16, want: u16) -> u16 {
    want.min(20.max(screen_w.saturating_sub(2)))
        .max(want.min(screen_w.saturating_sub(8)))
}

/// One key and what it does, e.g. `↵ play`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub keys: Cow<'static, str>,
    pub label: &'static str,
    /// The key a click presses; `None` for display-only clusters like `←→`.
    pub press: Option<KeyCode>,
}

impl Hint {
    /// A clickable hint for one key.
    pub fn key(code: KeyCode, label: &'static str) -> Self {
        Self {
            keys: key_name(code),
            label,
            press: Some(code),
        }
    }

    /// A display-only hint for a key cluster.
    pub fn show(keys: &'static str, label: &'static str) -> Self {
        Self {
            keys: Cow::Borrowed(keys),
            label,
            press: None,
        }
    }
}

/// How a key is written in hints and help.
pub fn key_name(code: KeyCode) -> Cow<'static, str> {
    Cow::Borrowed(match code {
        KeyCode::Enter => "↵",
        KeyCode::Esc => "esc",
        KeyCode::Tab => "tab",
        KeyCode::BackTab => "shift+tab",
        KeyCode::Backspace => "⌫",
        KeyCode::Up => "↑",
        KeyCode::Down => "↓",
        KeyCode::Left => "←",
        KeyCode::Right => "→",
        KeyCode::PageUp => "pgup",
        KeyCode::PageDown => "pgdn",
        KeyCode::Home => "home",
        KeyCode::End => "end",
        KeyCode::Char(' ') => "space",
        KeyCode::Char(c) => return Cow::Owned(c.to_string()),
        _ => "?",
    })
}

/// Greedy placement of hints into rows of `width` cells, 3 cells apart.
fn hint_layout(hints: &[Hint], width: u16) -> Vec<Position> {
    const GAP: u16 = 3;
    let mut out = Vec::with_capacity(hints.len());
    let (mut x, mut y) = (0u16, 0u16);
    for hint in hints {
        let w = hint_width(hint);
        if x > 0 && x + GAP + w > width {
            x = 0;
            y += 1;
        } else if x > 0 {
            x += GAP;
        }
        out.push(Position::new(x, y));
        x += w;
    }
    out
}

fn hint_width(hint: &Hint) -> u16 {
    let label = if hint.label.is_empty() {
        0
    } else {
        text::width(hint.label) + 1
    };
    // The keycap pads the key by a cell each side.
    (text::width(&hint.keys) + 2 + label) as u16
}

/// Rows the hint bar needs at `width`.
pub fn hints_height(hints: &[Hint], width: u16) -> u16 {
    hint_layout(hints, width).last().map_or(0, |p| p.y + 1)
}

fn underline(line: Line<'_>) -> Line<'_> {
    line.patch_style(Style::new().add_modifier(Modifier::UNDERLINED))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_wrap_to_width() {
        let hints = [
            Hint::key(KeyCode::Enter, "play"),
            Hint::key(KeyCode::Char('n'), "new"),
            Hint::key(KeyCode::Char('?'), "help"),
        ];
        // "[↵] play   [n] new   [?] help" is 29 cells, keycaps included.
        assert_eq!(hints_height(&hints, 29), 1);
        assert_eq!(hints_height(&hints, 28), 2);
        assert_eq!(hints_height(&[], 22), 0);
    }

    #[test]
    fn card_width_keeps_margins_and_a_floor() {
        assert_eq!(card_width(120, 56), 56);
        assert_eq!(card_width(60, 56), 56);
        assert_eq!(card_width(40, 56), 38);
        assert_eq!(card_width(10, 56), 20);
    }

    #[test]
    fn anchored_cards_stay_on_screen() {
        let screen = Rect::new(0, 0, 80, 24);
        let r = card_rect(screen, 30, 5, Some(Position::new(70, 20)));
        assert_eq!((r.x, r.y, r.width, r.height), (50, 12, 30, 12));
    }
}
