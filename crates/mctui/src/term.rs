//! Terminal setup and teardown.
//!
//! [`Tui::enter`] switches the terminal into full-screen raw mode with mouse
//! reporting; dropping the [`Tui`] (or a panic, which then exits) puts
//! everything back.

use std::io::{self, Stdout, Write};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement,
};
use crossterm::{cursor, execute, queue};
use ratatui::backend::CrosstermBackend;
use ratatui::{Frame, Terminal};

pub type Backend = CrosstermBackend<Stdout>;

pub struct Tui {
    terminal: Terminal<Backend>,
    _restore: Restore,
}

/// Whether we pushed keyboard flags, so restoring pops only our own.
static KEYBOARD_PUSHED: AtomicBool = AtomicBool::new(false);

/// Restores the terminal when dropped — including when setup fails halfway.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        restore();
    }
}

impl Tui {
    /// Takes over the terminal. `ask_keyboard`: whether to ask it about the
    /// keyboard protocol (a terminal that answers nothing makes that wait).
    pub fn enter(ask_keyboard: bool) -> io::Result<Self> {
        install_panic_hook();
        // Unambiguous Esc and modifier keys where the terminal supports it.
        let enhanced = ask_keyboard && supports_keyboard_enhancement().unwrap_or(false);
        let restore = Restore;
        setup(enhanced)?;
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok(Self {
            terminal,
            _restore: restore,
        })
    }

    /// Hands the terminal back to the shell and stops, like Ctrl+Z does
    /// elsewhere; takes it over again when resumed (`fg`).
    #[cfg(unix)]
    pub fn suspend(&mut self) -> io::Result<()> {
        let enhanced = KEYBOARD_PUSHED.load(Ordering::Relaxed);
        restore();
        rustix::process::kill_current_process_group(rustix::process::Signal::TSTP)?;
        // Resumed. The input reader owns the terminal's replies, so nothing
        // here may query it: reuse what we learned about the keyboard, and
        // start a fresh frame (ratatui's `clear` asks where the cursor is).
        setup(enhanced)?;
        execute!(io::stdout(), Clear(ClearType::All))?;
        self.terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok(())
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

/// Raw mode, the alternate screen, mouse, paste and focus reporting, and
/// (`enhanced`) the keyboard protocol.
fn setup(enhanced: bool) -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        EnableFocusChange
    )?;
    if enhanced {
        execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
        KEYBOARD_PUSHED.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Puts the terminal back the way we found it. Safe to call more than once.
fn restore() {
    let mut out = io::stdout();
    // Best effort: each step runs even if an earlier one failed.
    if KEYBOARD_PUSHED.swap(false, Ordering::Relaxed) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
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

/// Restores the terminal before a panic message prints, so it's readable,
/// then exits: a panic anywhere, even in background work, is a bug, and
/// carrying on would draw over the restored shell.
fn install_panic_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
            std::process::exit(101);
        }));
    });
}
