//! Narrowing a list by typing: `/` opens the field over the list's heading,
//! ↵ keeps the query and hands the keys back to the list, esc drops it.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;

use super::input::TextInput;
use super::{Canvas, text, widgets};

/// A list's filter.
#[derive(Debug, Default)]
pub enum Filter {
    #[default]
    Off,
    /// Typing a query; keys go to the field.
    Editing(TextInput),
    /// A query narrows the list; keys work as usual.
    Applied(TextInput),
}

/// What a key did to a filter being typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKey {
    /// The query changed: narrow the list again.
    Changed,
    /// ↵: typing stopped, keeping the query (a blank one turns it off).
    Committed,
    /// esc: the filter is off.
    Cleared,
    /// ↑ or ↓: move through the matches by this much.
    Step(isize),
    /// Used up without changing the query (the cursor moved, say).
    Consumed,
}

impl Filter {
    pub fn query(&self) -> &str {
        match self {
            Filter::Off => "",
            Filter::Editing(q) | Filter::Applied(q) => q.as_str(),
        }
    }

    pub fn is_editing(&self) -> bool {
        matches!(self, Filter::Editing(_))
    }

    /// Starts typing, from the query there is.
    pub fn edit(&mut self) {
        *self = Filter::Editing(TextInput::new(self.query()));
    }

    /// Stops typing, keeping what was typed. Returns whether it was being
    /// typed.
    pub fn commit(&mut self) -> bool {
        let Filter::Editing(input) = self else {
            return false;
        };
        let query = std::mem::take(input);
        *self = if query.as_str().trim().is_empty() {
            Filter::Off
        } else {
            Filter::Applied(query)
        };
        true
    }

    /// A key while typing: it edits the query, except ↵ (keep it), esc (drop
    /// it) and ↑↓ (move through the matches). `None` when not typing.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<FilterKey> {
        let Filter::Editing(input) = self else {
            return None;
        };
        Some(match key.code {
            KeyCode::Esc => {
                *self = Filter::Off;
                FilterKey::Cleared
            }
            KeyCode::Enter => {
                self.commit();
                FilterKey::Committed
            }
            KeyCode::Up => FilterKey::Step(-1),
            KeyCode::Down => FilterKey::Step(1),
            _ if input.handle_key(key) == Some(true) => FilterKey::Changed,
            _ => FilterKey::Consumed,
        })
    }

    /// Pasted text while typing. Returns whether the query changed, or `None`
    /// when not typing.
    pub fn paste(&mut self, text: &str) -> Option<bool> {
        match self {
            Filter::Editing(input) => Some(input.insert(text)),
            Filter::Off | Filter::Applied(_) => None,
        }
    }

    /// Draws a filtered list's heading across `row` (a pane's width): the
    /// search field while typing; otherwise `label` as the section label,
    /// how many rows are showing, and the query narrowing them —
    /// `INSTANCES   2 of 6   “sur”`. Returns the x after the label and its
    /// count, when it drew them.
    pub fn heading(
        &self,
        cv: &mut Canvas<'_>,
        row: Rect,
        label: &str,
        (shown, total): (usize, usize),
        focused: bool,
        placeholder: &str,
    ) -> Option<u16> {
        let count = if shown == total {
            total.to_string()
        } else {
            format!("{shown} of {total}")
        };
        let title = Rect::new(row.x + 2, row.y, row.width.saturating_sub(4), 1);
        let count = match self {
            Filter::Editing(input) => {
                widgets::search(cv, row, input, placeholder, Some(&count), true, None);
                return None;
            }
            Filter::Applied(input) => {
                let query = text::truncate(input.as_str(), 16);
                format!("{count}   “{query}”")
            }
            Filter::Off => count,
        };
        Some(widgets::section(cv, title, label, Some(&count), focused))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(filter: &mut Filter, code: KeyCode) -> Option<FilterKey> {
        filter.handle_key(KeyEvent::from(code))
    }

    #[test]
    fn typing_then_enter_keeps_the_query() {
        let mut filter = Filter::default();
        assert_eq!(press(&mut filter, KeyCode::Char('x')), None, "not typing");
        filter.edit();
        assert_eq!(
            press(&mut filter, KeyCode::Char('s')),
            Some(FilterKey::Changed)
        );
        assert_eq!(press(&mut filter, KeyCode::Left), Some(FilterKey::Consumed));
        assert_eq!(press(&mut filter, KeyCode::Down), Some(FilterKey::Step(1)));
        assert_eq!(
            press(&mut filter, KeyCode::Enter),
            Some(FilterKey::Committed)
        );
        assert!(matches!(&filter, Filter::Applied(q) if q.as_str() == "s"));
        filter.edit();
        assert_eq!(filter.query(), "s", "editing again starts from the query");
        assert_eq!(press(&mut filter, KeyCode::Esc), Some(FilterKey::Cleared));
        assert!(matches!(filter, Filter::Off));
    }

    #[test]
    fn a_blank_query_turns_the_filter_off() {
        let mut filter = Filter::default();
        filter.edit();
        assert_eq!(filter.paste("  "), Some(true));
        assert!(filter.commit());
        assert!(matches!(filter, Filter::Off));
        assert!(!filter.commit(), "nothing to commit");
        assert_eq!(filter.paste("x"), None);
    }
}
