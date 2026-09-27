//! Screens: what the app shows between the header and the footer.
//!
//! Home is always at the bottom of the stack; other screens are pushed over it
//! and popped when done. The app owns the shared chrome (header, toasts,
//! footer, overlays) and asks the active screen for its parts.

use std::time::Instant;

use crossterm::event::KeyEvent;
use ratatui::layout::{Position, Rect};

use crate::app::Action;
use crate::data::Data;
use crate::msg::Msg;
use crate::overlay::{HelpGroup, PaletteItem};
use crate::services::Services;
use crate::task::Tasks;
use crate::theme::Theme;
use crate::ui::{Canvas, Hint, Target};

/// Identifies a screen instance, so background work reports to the screen
/// that started it and is dropped if that screen is gone.
pub type ScreenId = u64;

/// How a screen dealt with an input.
#[derive(Debug)]
pub enum Handled {
    /// Not the screen's business; the app may use it.
    Ignored,
    /// Used; redraw.
    Consumed,
    /// Used, and the app should do this.
    Action(Action),
}

impl Handled {
    /// `Consumed` if `changed`, else `Ignored`.
    pub fn changed(changed: bool) -> Self {
        if changed {
            Self::Consumed
        } else {
            Self::Ignored
        }
    }
}

/// A mouse gesture on one of a screen's targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouse {
    Click { double: bool },
    RightClick(Position),
    Scroll { down: bool },
}

/// What a screen may use while handling input.
#[expect(
    dead_code,
    reason = "tasks, services, theme and now serve the screens being built"
)]
pub struct Ctx<'a> {
    pub data: &'a mut Data,
    /// Clients for the online services.
    pub services: &'a Services,
    /// Starts background work that reports back to this screen.
    pub tasks: &'a Tasks,
    pub theme: &'a Theme,
    pub now: Instant,
}

/// One screen of the app.
pub trait Screen: std::fmt::Debug {
    /// Breadcrumbs after the wordmark, e.g. `["Survival 1.21", "Mods"]`.
    fn crumbs(&self, data: &Data) -> Vec<String>;

    /// The footer: only keys that work right now.
    fn hints(&self, data: &Data) -> Vec<Hint>;

    /// The `?` card: a title and the key groups.
    fn help(&self) -> (&'static str, Vec<HelpGroup>);

    /// The screen's commands for the `:` palette.
    fn palette(&self, _data: &Data) -> Vec<PaletteItem> {
        Vec::new()
    }

    /// Whether keys are going into a text field, so `?`, `:` and `q` type.
    fn captures_text(&self) -> bool {
        false
    }

    /// The area the screen draws in changed.
    fn set_body(&mut self, body: Rect);

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled;

    fn handle_mouse(&mut self, _target: &Target, _mouse: Mouse, _cx: &mut Ctx<'_>) -> Handled {
        Handled::Ignored
    }

    fn handle_paste(&mut self, _text: &str, _cx: &mut Ctx<'_>) -> Handled {
        Handled::Ignored
    }

    /// A background task this screen started reported back.
    fn handle_msg(&mut self, _msg: Msg, _cx: &mut Ctx<'_>) -> Handled {
        Handled::Ignored
    }

    /// Instances changed on disk (or were edited elsewhere in the app).
    fn reload(&mut self, _data: &Data) {}

    /// When the screen next needs to redraw on its own (a spinner frame).
    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    /// Time passed. Returns whether to redraw.
    fn tick(&mut self, _now: Instant) -> bool {
        false
    }

    fn render(&self, cv: &mut Canvas<'_>, data: &Data);
}
