//! Mouse hit-testing.
//!
//! Every frame, rendering records the screen rectangles that respond to the
//! mouse. Input handling then resolves a pointer position against the last
//! frame's map, so what you click is always exactly what you saw.

use crossterm::event::KeyCode;
use ratatui::layout::{Position, Rect};

use crate::{home, mods, overlay, wizard};

/// Something on screen the mouse can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A key hint or button: clicking it presses the key.
    Key(KeyCode),
    /// Everything outside an open overlay's card. Clicking it closes the overlay.
    Backdrop,
    /// The signed-in account in the header: opens Accounts.
    Account,
    Overlay(overlay::Target),
    /// A form field, by position (one form is on screen at a time).
    Field(usize),
    /// A row of a [`super::list::List`], by the list's id and the row's key.
    Row {
        list: &'static str,
        key: String,
    },
    Home(home::Target),
    /// The launch screen's buttons and log.
    Play(crate::play::PlayTarget),
    Wizard(wizard::Target),
    Mods(mods::Target),
}

impl Target {
    /// Whether this is a list row, where a double-click means "open".
    pub fn is_row(&self) -> bool {
        matches!(
            self,
            Target::Row { .. } | Target::Home(home::Target::Row(_))
        )
    }
}

/// The clickable regions of one frame. Later regions sit on top of earlier ones.
#[derive(Debug, Default)]
pub struct Hits {
    regions: Vec<(Rect, Target)>,
}

impl Hits {
    pub fn push(&mut self, area: Rect, target: Target) {
        if !area.is_empty() {
            self.regions.push((area, target));
        }
    }

    /// The topmost target under `pos`.
    pub fn at(&self, pos: Position) -> Option<&Target> {
        self.regions
            .iter()
            .rev()
            .find(|(area, _)| area.contains(pos))
            .map(|(_, target)| target)
    }

    #[cfg(test)]
    pub fn find(&self, target: &Target) -> Option<Rect> {
        self.regions
            .iter()
            .rev()
            .find(|(_, t)| t == target)
            .map(|(area, _)| *area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_regions_win() {
        let mut hits = Hits::default();
        hits.push(Rect::new(0, 0, 10, 10), Target::Backdrop);
        hits.push(Rect::new(2, 2, 3, 1), Target::Key(KeyCode::Enter));
        assert_eq!(
            hits.at(Position::new(3, 2)),
            Some(&Target::Key(KeyCode::Enter))
        );
        assert_eq!(hits.at(Position::new(3, 3)), Some(&Target::Backdrop));
        assert_eq!(hits.at(Position::new(20, 3)), None);
    }
}
