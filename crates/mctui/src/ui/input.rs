//! A single-line text field with readline-style editing.
//!
//! Adapted from herdr's `TextEditor` (<https://github.com/herdrdev/herdr>,
//! Apache-2.0): a byte cursor that always sits on a grapheme boundary, word
//! motion, and a one-slot kill buffer.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::Canvas;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    text: String,
    /// Byte offset, always on a grapheme boundary.
    cursor: usize,
    /// The last text cut with ctrl+u/k/w or alt+d, for ctrl+y.
    killed: String,
}

impl TextInput {
    pub fn new(text: &str) -> Self {
        let mut input = Self::default();
        input.insert(text);
        input
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Inserts pasted or typed text at the cursor. Newlines and tabs become
    /// spaces; other control characters are dropped.
    pub fn insert(&mut self, text: &str) -> bool {
        let clean: String = text
            .chars()
            .filter_map(|c| match c {
                '\r' | '\n' | '\t' => Some(' '),
                c if c.is_control() => None,
                c => Some(c),
            })
            .collect();
        if clean.is_empty() {
            return false;
        }
        self.text.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
        self.snap_cursor();
        true
    }

    /// Handles an editing key. Returns `None` when the key isn't an editing
    /// key (so the caller can use it), otherwise whether the text changed.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<bool> {
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        let alt = key.modifiers == KeyModifiers::ALT;
        let plain = key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
        let before = self.text.len();

        let motion = match key.code {
            KeyCode::Left if plain => Some(self.prev()),
            KeyCode::Char('b') if ctrl => Some(self.prev()),
            KeyCode::Right if plain => Some(self.next()),
            KeyCode::Char('f') if ctrl => Some(self.next()),
            KeyCode::Home if plain => Some(0),
            KeyCode::Char('a') if ctrl => Some(0),
            KeyCode::End if plain => Some(self.text.len()),
            KeyCode::Char('e') if ctrl => Some(self.text.len()),
            KeyCode::Left if alt || ctrl => Some(self.word_boundary(true)),
            KeyCode::Char('b') if alt => Some(self.word_boundary(true)),
            KeyCode::Right if alt || ctrl => Some(self.word_boundary(false)),
            KeyCode::Char('f') if alt => Some(self.word_boundary(false)),
            _ => None,
        };
        if let Some(pos) = motion {
            self.cursor = pos;
            return Some(false);
        }

        match key.code {
            KeyCode::Backspace if plain => self.remove(self.prev(), self.cursor, false),
            KeyCode::Char('h') if ctrl => self.remove(self.prev(), self.cursor, false),
            KeyCode::Delete if plain => self.remove(self.cursor, self.next(), false),
            KeyCode::Char('d') if ctrl => self.remove(self.cursor, self.next(), false),
            KeyCode::Char('u') if ctrl => self.remove(0, self.cursor, true),
            KeyCode::Char('k') if ctrl => self.remove(self.cursor, self.text.len(), true),
            KeyCode::Char('w') if ctrl => self.remove(self.word_boundary(true), self.cursor, true),
            KeyCode::Backspace if ctrl || alt => {
                self.remove(self.word_boundary(true), self.cursor, true);
            }
            KeyCode::Char('d') if alt => self.remove(self.cursor, self.word_boundary(false), true),
            KeyCode::Char('y') if ctrl => {
                let killed = self.killed.clone();
                self.insert(&killed);
            }
            KeyCode::Char(c) if plain => {
                self.insert(c.encode_utf8(&mut [0; 4]));
            }
            _ => return None,
        }
        Some(self.text.len() != before)
    }

    /// Draws the field on one row, scrolled so the cursor stays visible, with
    /// `placeholder` shown when empty. Places the terminal cursor when `focused`.
    pub fn render(
        &self,
        cv: &mut Canvas<'_>,
        area: Rect,
        style: Style,
        placeholder: &str,
        focused: bool,
    ) {
        if area.is_empty() {
            return;
        }
        if self.text.is_empty() {
            let dim = Style::new().fg(cv.theme().muted);
            cv.text(area.x, area.y, placeholder, area.width, dim);
            if focused {
                cv.cursor = Some(Position::new(area.x, area.y));
            }
            return;
        }
        let (visible, cursor_x) = self.viewport(area.width);
        cv.text(area.x, area.y, visible, area.width, style);
        if focused {
            cv.cursor = Some(Position::new(area.x + cursor_x, area.y));
        }
    }

    /// The slice of text that fits in `width` cells around the cursor, and the
    /// cursor's column within it.
    fn viewport(&self, width: u16) -> (&str, u16) {
        let width = usize::from(width);
        let mut start = self.cursor;
        let mut cells = 0;
        for (i, g) in self.text[..self.cursor].grapheme_indices(true).rev() {
            if cells + g.width() >= width {
                break;
            }
            start = i;
            cells += g.width();
        }
        let mut end = self.cursor;
        let mut used = cells;
        for (i, g) in self.text[self.cursor..].grapheme_indices(true) {
            used += g.width();
            if used > width {
                break;
            }
            end = self.cursor + i + g.len();
        }
        (&self.text[start..end], cells as u16)
    }

    fn prev(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.cursor
            + self.text[self.cursor..]
                .graphemes(true)
                .next()
                .map_or(0, str::len)
    }

    fn word_boundary(&self, backward: bool) -> usize {
        fn class(g: &str) -> u8 {
            match g.chars().next() {
                Some(c) if c.is_whitespace() => 0,
                Some(c) if c.is_alphanumeric() || c == '_' => 1,
                _ => 2,
            }
        }
        let mut boundary = self.cursor;
        let mut run = None;
        if backward {
            for (i, g) in self.text[..self.cursor].grapheme_indices(true).rev() {
                let c = class(g);
                if run.is_some_and(|r| r != 0 && r != c) {
                    break;
                }
                run = Some(c);
                boundary = i;
            }
        } else {
            for (i, g) in self.text[self.cursor..].grapheme_indices(true) {
                let c = class(g);
                if run.is_some_and(|r| r != 0 && r != c) {
                    break;
                }
                run = Some(c);
                boundary = self.cursor + i + g.len();
            }
        }
        boundary
    }

    fn remove(&mut self, start: usize, end: usize, kill: bool) {
        if start == end {
            return;
        }
        if kill {
            self.killed = self.text[start..end].to_owned();
        }
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.snap_cursor();
    }

    /// Edits can merge graphemes around the cursor; move it forward to the
    /// next boundary so it never sits inside one.
    fn snap_cursor(&mut self) {
        self.cursor = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|&i| i >= self.cursor)
            .unwrap_or(self.text.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(input: &mut TextInput, code: KeyCode, mods: KeyModifiers) -> Option<bool> {
        input.handle_key(KeyEvent::new(code, mods))
    }

    fn typed(s: &str) -> TextInput {
        let mut input = TextInput::default();
        for c in s.chars() {
            press(&mut input, KeyCode::Char(c), KeyModifiers::NONE);
        }
        input
    }

    #[test]
    fn typing_and_backspace() {
        let mut input = typed("héllo");
        assert_eq!(input.as_str(), "héllo");
        assert_eq!(
            press(&mut input, KeyCode::Backspace, KeyModifiers::NONE),
            Some(true)
        );
        assert_eq!(input.as_str(), "héll");
        // Shifted letters are text, not shortcuts.
        press(&mut input, KeyCode::Char('O'), KeyModifiers::SHIFT);
        assert_eq!(input.as_str(), "héllO");
    }

    #[test]
    fn word_kill_and_yank() {
        let mut input = typed("survival world 2");
        press(&mut input, KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(input.as_str(), "survival world ");
        press(&mut input, KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(input.as_str(), "survival ");
        press(&mut input, KeyCode::Char('a'), KeyModifiers::CONTROL);
        press(&mut input, KeyCode::Char('y'), KeyModifiers::CONTROL);
        assert_eq!(input.as_str(), "world survival ");
    }

    #[test]
    fn line_kills() {
        let mut input = typed("abc def");
        press(&mut input, KeyCode::Left, KeyModifiers::NONE);
        press(&mut input, KeyCode::Left, KeyModifiers::NONE);
        press(&mut input, KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(input.as_str(), "abc d");
        press(&mut input, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(input.as_str(), "");
    }

    #[test]
    fn non_editing_keys_pass_through() {
        let mut input = typed("x");
        assert_eq!(press(&mut input, KeyCode::Enter, KeyModifiers::NONE), None);
        assert_eq!(press(&mut input, KeyCode::Esc, KeyModifiers::NONE), None);
        assert_eq!(press(&mut input, KeyCode::Up, KeyModifiers::NONE), None);
        assert_eq!(
            press(&mut input, KeyCode::Char('c'), KeyModifiers::CONTROL),
            None
        );
    }

    #[test]
    fn paste_flattens_newlines() {
        let mut input = TextInput::default();
        input.insert("a\nb\tc\u{7}");
        assert_eq!(input.as_str(), "a b c");
    }

    #[test]
    fn viewport_follows_the_cursor() {
        let input = typed("abcdefghij");
        let (visible, x) = input.viewport(5);
        assert_eq!(visible, "ghij");
        assert_eq!(x, 4);
    }
}
