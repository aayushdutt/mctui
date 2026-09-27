//! A scrollable list of one-line rows with a blank line between them.
//!
//! Rows are identified by a string key, so the selection (and a click on the
//! frame you saw) survives the rows being rebuilt — a refresh, a filter.
//! ↑↓ / jk move and stop at the ends; pgup/pgdn page; g/G or home/end jump.

use std::rc::Rc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{Canvas, Target, widgets};

/// Lines per row: the row and the blank line after it.
const PITCH: u16 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct List {
    /// Which list this is, so its rows' click targets are its own.
    id: &'static str,
    /// Shared with the rows' click targets, so drawing a frame copies none.
    keys: Vec<Rc<str>>,
    selected: usize,
    offset: usize,
    /// Rows the list may draw in, set by the last layout.
    height: u16,
}

impl List {
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            ..Self::default()
        }
    }

    /// Replaces the rows, keeping the same key selected if it's still there.
    pub fn set_keys<K: Into<Rc<str>>>(&mut self, keys: impl IntoIterator<Item = K>) {
        let before = self.keys.get(self.selected).cloned();
        self.keys = keys.into_iter().map(Into::into).collect();
        self.selected = before
            .and_then(|k| self.keys.iter().position(|x| *x == k))
            .unwrap_or(0)
            .min(self.keys.len().saturating_sub(1));
        self.scroll_into_view();
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_key(&self) -> Option<&str> {
        self.keys.get(self.selected).map(|k| &**k)
    }

    /// Selects the row with `key`. Returns whether the selection moved.
    pub fn select_key(&mut self, key: &str) -> bool {
        self.keys
            .iter()
            .position(|k| **k == *key)
            .is_some_and(|i| self.select(i))
    }

    /// Selects row `i` (clamped). Returns whether the selection moved.
    pub fn select(&mut self, i: usize) -> bool {
        let i = i.min(self.keys.len().saturating_sub(1));
        let moved = i != self.selected;
        self.selected = i;
        self.scroll_into_view();
        moved
    }

    pub fn step(&mut self, delta: isize) -> bool {
        self.select(self.selected.saturating_add_signed(delta))
    }

    /// The click target for the row with `key`.
    fn target(&self, key: &Rc<str>) -> Target {
        Target::Row {
            list: self.id,
            key: Rc::clone(key),
        }
    }

    /// If `target` is one of this list's rows, its key.
    pub fn key_of<'t>(&self, target: &'t Target) -> Option<&'t str> {
        match target {
            Target::Row { list, key } if *list == self.id => Some(&**key),
            _ => None,
        }
    }

    /// The area rows are drawn in changed.
    pub fn set_height(&mut self, height: u16) {
        self.height = height;
        self.scroll_into_view();
    }

    fn visible(&self) -> usize {
        usize::from(self.height.saturating_sub(1).div_ceil(PITCH)).max(1)
    }

    fn scroll_into_view(&mut self) {
        let visible = self.visible();
        let max_offset = self.keys.len().saturating_sub(visible);
        self.offset = self
            .offset
            .max((self.selected + 1).saturating_sub(visible))
            .min(self.selected)
            .min(max_offset);
    }

    /// Movement keys. Returns whether the key was one (moved or not, it's
    /// used up so it can't fall through to another pane).
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<bool> {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        let page = isize::try_from(self.visible()).unwrap_or(1);
        Some(match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::PageUp => self.step(-page),
            KeyCode::PageDown => self.step(page),
            KeyCode::Home | KeyCode::Char('g') => self.select(0),
            KeyCode::End | KeyCode::Char('G') => self.select(usize::MAX),
            _ => return None,
        })
    }

    /// Draws the visible rows in `area`: each gets its band (selection only
    /// when `focused`, hover always), a click target, and `row` draws its
    /// content in the inset area it's given. The last line of `area` is kept
    /// for the "↓ N more" hint; the line above `area` gets "↑ N more".
    pub fn render(
        &self,
        cv: &mut Canvas<'_>,
        area: Rect,
        focused: bool,
        mut row: impl FnMut(&mut Canvas<'_>, Rect, usize, bool, bool),
    ) {
        let t = *cv.theme();
        let more = Style::new().fg(t.faint);
        if self.offset > 0 && area.y > 0 {
            cv.text(
                area.x + 2,
                area.y - 1,
                &format!("↑ {} more", self.offset),
                area.width,
                more,
            );
        }
        let visible = self.visible();
        for (i, key) in self.keys.iter().enumerate().skip(self.offset).take(visible) {
            let y = area.y + (i - self.offset) as u16 * PITCH;
            if y >= area.bottom() {
                break;
            }
            let band = Rect::new(area.x, y, area.width, 1);
            let target = self.target(key);
            let hovered = cv.hovered(&target);
            let selected = focused && i == self.selected;
            let inner = widgets::row(cv, band, selected, hovered);
            row(cv, inner, i, selected, hovered);
            cv.click(band, target);
        }
        let below = self.keys.len().saturating_sub(self.offset + visible);
        if below > 0 && area.height > 0 {
            let y = area.bottom() - 1;
            cv.text(area.x + 2, y, &format!("↓ {below} more"), area.width, more);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(n: usize, height: u16) -> List {
        let mut list = List::new("test");
        list.set_height(height);
        list.set_keys((0..n).map(|i| i.to_string()));
        list
    }

    fn press(list: &mut List, code: KeyCode) -> Option<bool> {
        list.handle_key(KeyEvent::from(code))
    }

    #[test]
    fn moves_stop_at_the_ends_and_scroll() {
        let mut l = list(10, 7); // 3 rows fit, plus the "more" line.
        assert_eq!(press(&mut l, KeyCode::Up), Some(false));
        for _ in 0..5 {
            press(&mut l, KeyCode::Down);
        }
        assert_eq!(l.selected(), 5);
        assert_eq!(l.offset, 3);
        press(&mut l, KeyCode::Char('G'));
        assert_eq!((l.selected(), l.offset), (9, 7));
        assert_eq!(press(&mut l, KeyCode::Char('x')), None);
    }

    #[test]
    fn selection_follows_its_key_through_a_rebuild() {
        let mut l = list(5, 20);
        l.select_key("3");
        l.set_keys(["9", "3"]);
        assert_eq!(l.selected_key(), Some("3"));
        l.set_keys(["7"]);
        assert_eq!(l.selected_key(), Some("7"), "falls back to the first row");
    }

    #[test]
    fn click_targets_belong_to_their_list() {
        let a = List::new("a");
        let b = List::new("b");
        let target = a.target(&Rc::from("x"));
        assert_eq!(a.key_of(&target), Some("x"));
        assert_eq!(b.key_of(&target), None);
        assert!(target.is_row());
    }
}
