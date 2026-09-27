//! Terminal setup and teardown.
//!
//! [`Tui::enter`] switches the terminal into full-screen raw mode with mouse
//! reporting; dropping the [`Tui`] (or a panic) puts everything back.

use std::io::{self, Stdout, Write};
use std::sync::Once;

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen,
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement,
};
use crossterm::{cursor, execute, queue};
use ratatui::backend::CrosstermBackend;
use ratatui::{Frame, Terminal};

pub type Backend = CrosstermBackend<Stdout>;

pub struct Tui {
    terminal: Terminal<Backend>,
    _restore: Restore,
}

/// Restores the terminal when dropped — including when setup fails halfway.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        restore();
    }
}

impl Tui {
    pub fn enter() -> io::Result<Self> {
        install_panic_hook();
        enable_raw_mode()?;
        let restore = Restore;
        let mut out = io::stdout();
        execute!(
            out,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            EnableFocusChange
        )?;
        // Unambiguous Esc and modifier keys where the terminal supports it.
        if supports_keyboard_enhancement().unwrap_or(false) {
            execute!(
                out,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }
        let terminal = Terminal::new(CrosstermBackend::new(out))?;
        Ok(Self {
            terminal,
            _restore: restore,
        })
    }

    pub fn size(&self) -> io::Result<ratatui::layout::Size> {
        self.terminal.size()
    }

    /// Draws a frame inside a synchronized update, so the terminal shows it
    /// all at once instead of tearing.
    pub fn draw(&mut self, render: impl FnOnce(&mut Frame<'_>)) -> io::Result<()> {
        queue!(self.terminal.backend_mut(), BeginSynchronizedUpdate)?;
        self.terminal.draw(render)?;
        execute!(self.terminal.backend_mut(), EndSynchronizedUpdate)
    }
}

/// Puts the terminal back the way we found it. Safe to call more than once.
fn restore() {
    let mut out = io::stdout();
    // Best effort: each step runs even if an earlier one failed.
    let _ = execute!(out, PopKeyboardEnhancementFlags);
    let _ = execute!(
        out,
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture
    );
    let _ = execute!(out, LeaveAlternateScreen, cursor::Show);
    let _ = disable_raw_mode();
    let _ = out.flush();
}

/// Restores the terminal before a panic message prints, so it's readable.
fn install_panic_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
    });
}
