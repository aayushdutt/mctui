//! mctui: a fast, mouse-friendly Minecraft launcher for your terminal.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod app;
mod data;
mod format;
mod fuzzy;
mod home;
mod overlay;
mod term;
#[cfg(test)]
mod testing;
mod theme;
mod ui;

use std::io::IsTerminal as _;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use crossterm::event::Event;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use mctui_core::paths::DataDir;
use ratatui::layout::Rect;
use tokio::sync::mpsc;

use crate::app::App;
use crate::data::Data;
use crate::term::Tui;
use crate::theme::{Terminal, Theme};

/// Never draw more often than this: floods of events (drags, progress) still
/// coalesce into ~60 frames a second.
const FRAME: Duration = Duration::from_millis(16);

/// At most this many queued events are handled between frames.
const MAX_BATCH: usize = 512;

fn main() -> anyhow::Result<()> {
    if std::env::args()
        .skip(1)
        .any(|a| matches!(a.as_str(), "--version" | "-v" | "version"))
    {
        println!("mctui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let dir = DataDir::resolve().context("finding the data folder")?;
    init_logging(&dir);
    let (data, notices) = Data::load(&dir);

    // Ask the terminal about its background before anything else reads stdin.
    let terminal = if std::io::stdout().is_terminal() {
        Terminal::detect()
    } else {
        Terminal::default()
    };
    let theme = Theme::resolve(&data.config.theme, terminal);

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run(data, notices, theme))
}

async fn run(data: Data, notices: Vec<String>, theme: Theme) -> anyhow::Result<()> {
    let mut tui = Tui::enter()?;
    let size = tui.size()?;
    let mut app = App::new(
        data,
        notices,
        theme,
        Rect::new(0, 0, size.width, size.height),
    );
    let mut events = spawn_input_reader();
    let tz = TimeZone::system();
    let mut prof = RenderProf::from_env();

    let mut dirty = true;
    let mut next_frame = Instant::now();
    while !app.should_quit() {
        if dirty && Instant::now() >= next_frame {
            let started = Instant::now();
            let cx = app.cx(Timestamp::now(), tz.clone());
            let mut out = None;
            tui.draw(|frame| {
                let (hits, cursor) = app.render(frame, &cx);
                if let Some(pos) = cursor {
                    frame.set_cursor_position(pos);
                }
                out = Some(hits);
            })?;
            dirty = out.is_some_and(|hits| app.set_hits(hits));
            next_frame = started + FRAME;
            prof.record(started.elapsed());
        }

        let wake = match (app.next_deadline(), dirty) {
            (Some(deadline), true) => Some(deadline.min(next_frame)),
            (None, true) => Some(next_frame),
            (deadline, false) => deadline,
        };
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break };
                dirty |= app.handle(event);
                // Handle what's already queued before drawing once — but not
                // forever, so a flood still gets frames.
                for _ in 0..MAX_BATCH {
                    let Ok(event) = events.try_recv() else { break };
                    dirty |= app.handle(event);
                }
            }
            () = sleep_until(wake) => {
                dirty |= app.tick(Instant::now());
            }
        }
    }
    Ok(())
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

/// Reads terminal events on a dedicated thread; the channel closes if reading fails.
fn spawn_input_reader() -> mpsc::UnboundedReceiver<Event> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    rx
}

/// Logs to `<data>/mctui.log` when `MCTUI_LOG` is set (e.g. `MCTUI_LOG=debug`).
fn init_logging(dir: &DataDir) {
    let Ok(filter) = std::env::var("MCTUI_LOG") else {
        return;
    };
    let path = dir.root().join("mctui.log");
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .init();
}

/// Frame timing stats, logged once a second when `MCTUI_RENDER_PROF` is set.
struct RenderProf {
    enabled: bool,
    since: Instant,
    frames: u32,
    total: Duration,
    worst: Duration,
}

impl RenderProf {
    fn from_env() -> Self {
        Self {
            enabled: std::env::var_os("MCTUI_RENDER_PROF").is_some(),
            since: Instant::now(),
            frames: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
        }
    }

    fn record(&mut self, took: Duration) {
        if !self.enabled {
            return;
        }
        self.frames += 1;
        self.total += took;
        self.worst = self.worst.max(took);
        if self.since.elapsed() >= Duration::from_secs(1) {
            tracing::info!(
                frames = self.frames,
                avg_us = (self.total / self.frames).as_micros(),
                max_us = self.worst.as_micros(),
                "render"
            );
            *self = Self::from_env();
        }
    }
}
