//! Floating cards over a dimmed screen: help, the command palette, menus and
//! confirmations. At most one is open at a time and it owns all input until it
//! closes.

mod confirm;
mod help;
mod menu;
mod palette;

pub use confirm::Confirm;
pub use help::{Help, HelpGroup};
pub use menu::{Menu, MenuItem};
pub use palette::{Palette, PaletteItem};

use crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::app::Action;
use crate::ui::{self, Canvas};

/// What an overlay's parts are, for the mouse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Inside the card but not on a control: absorbs clicks.
    Card,
    /// A palette or menu row, by position in what's shown.
    Row(usize),
    Yes,
    No,
}

/// What happens after an overlay handles input.
#[derive(Debug)]
pub enum Outcome {
    /// Still open (it may have changed).
    Stay,
    Close,
    /// Close, then do this.
    Run(Action),
}

#[derive(Debug)]
pub enum Overlay {
    Help(Help),
    Palette(Palette),
    Menu(Menu),
    Confirm(Confirm),
}

impl Overlay {
    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        match self {
            Self::Help(h) => h.handle_key(key),
            Self::Palette(p) => p.handle_key(key),
            Self::Menu(m) => m.handle_key(key),
            Self::Confirm(c) => c.handle_key(key),
        }
    }

    pub fn handle_paste(&mut self, text: &str) {
        if let Self::Palette(p) = self {
            p.paste(text);
        }
    }

    /// The pointer moved onto `target`. Lists follow the pointer; dialog
    /// buttons only light up — hovering must never arm a destructive Enter.
    /// Returns whether anything changed.
    pub fn hover(&mut self, target: &Target) -> bool {
        match (self, target) {
            (Self::Palette(p), Target::Row(i)) => p.select(*i),
            (Self::Menu(m), Target::Row(i)) => m.select(*i),
            _ => false,
        }
    }

    pub fn click(&mut self, target: &Target) -> Outcome {
        match (self, target) {
            (Self::Palette(p), Target::Row(i)) => {
                p.select(*i);
                p.run()
            }
            (Self::Menu(m), Target::Row(i)) => {
                m.select(*i);
                m.run()
            }
            (Self::Confirm(c), Target::Yes) => c.answer(true),
            (Self::Confirm(c), Target::No) => c.answer(false),
            _ => Outcome::Stay,
        }
    }

    /// The wheel moves the highlight in lists.
    pub fn scroll(&mut self, down: bool) -> bool {
        match self {
            Self::Palette(p) => p.step(down),
            Self::Menu(m) => m.step(down),
            Self::Help(h) => h.scroll_by(if down { 3 } else { -3 }),
            Self::Confirm(_) => false,
        }
    }

    /// Draws the overlay over `screen`, which already holds the frame beneath.
    pub fn render(&self, cv: &mut Canvas<'_>, screen: Rect) {
        cv.dim(screen);
        cv.click(screen, ui::Target::Backdrop);
        match self {
            Self::Help(h) => h.render(cv, screen),
            Self::Palette(p) => p.render(cv, screen),
            Self::Menu(m) => m.render(cv, screen),
            Self::Confirm(c) => c.render(cv, screen),
        }
    }
}

/// Draws a card, claims its area so clicks inside it don't reach the
/// backdrop, and draws its body with `draw`, confined to the card.
fn card(
    cv: &mut Canvas<'_>,
    rect: Rect,
    title: &str,
    accent: ratatui::style::Color,
    draw: impl FnOnce(&mut Canvas<'_>, Rect),
) {
    cv.click(rect, ui::Target::Overlay(Target::Card));
    let body = cv.card(rect, title, accent);
    cv.clipped(rect, |cv| draw(cv, body));
}
