//! mctui: a fast, mouse-friendly Minecraft launcher for your terminal.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod accounts;
mod app;
mod clipboard;
mod data;
mod edit;
mod format;
mod fuzzy;
mod home;
mod mods;
mod msg;
mod overlay;
mod play;
mod resource_packs;
mod screen;
mod services;
mod settings;
mod task;
mod term;
#[cfg(test)]
mod testing;
mod theme;
mod ui;
mod wizard;

use std::io::IsTerminal as _;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use mctui_core::net::Http;
use mctui_core::paths::DataDir;
use ratatui::layout::Rect;
use tokio::sync::mpsc;

use crate::app::App;
use crate::data::Data;
use crate::services::Services;
use crate::task::{Event, Spawner};
use crate::term::Tui;
use crate::theme::Terminal;

/// Never draw more often than this: floods of events (drags, progress) still
/// coalesce into ~60 frames a second.
const FRAME: Duration = Duration::from_millis(16);

/// At most this many queued events are handled between frames, for at most
/// this long.
const MAX_BATCH: usize = 512;
const MAX_BATCH_TIME: Duration = Duration::from_millis(8);

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

    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(run(data, notices, terminal));
    // Don't hold the shell prompt for work nobody will see (an unpack).
    runtime.shutdown_timeout(Duration::from_millis(300));
    result
}

async fn run(data: Data, notices: Vec<String>, terminal: Terminal) -> anyhow::Result<()> {
    let mut tui = Tui::enter(!terminal.silent)?;
    let size = tui.size()?;
    let (tx, mut events) = mpsc::unbounded_channel();
    spawn_input_reader(tx.clone());
    let spawner = Spawner::new(tx, tokio::runtime::Handle::current());
    let services = Services::new(Http::new()?, &data.dir);
    let area = Rect::new(0, 0, size.width, size.height);
    let mut app = App::new(data, notices, terminal, (spawner, services), area);
    app.start();
    let tz = TimeZone::system();
    let mut prof = RenderProf::from_env();

    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
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
            if let Some(text) = app.take_clipboard() {
                clipboard::copy(&text);
            }
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
                let batch = Instant::now();
                dirty |= app.handle(event);
                // Handle what's already queued before drawing once — but not
                // forever, so a flood still gets frames.
                for _ in 0..MAX_BATCH {
                    if batch.elapsed() >= MAX_BATCH_TIME {
                        break;
                    }
                    let Ok(event) = events.try_recv() else { break };
                    dirty |= app.handle(event);
                }
            }
            () = sleep_until(wake) => {
                dirty |= app.tick(Instant::now());
            }
            // Quitting normally is what puts the terminal back.
            () = &mut shutdown => break,
        }
        if app.take_suspend() {
            #[cfg(unix)]
            {
                tui.suspend()?;
                // The window may have changed size while we were away.
                let size = tui.size()?;
                app.handle(Event::Term(crossterm::event::Event::Resize(
                    size.width,
                    size.height,
                )));
            }
            dirty = true;
        }
    }
    Ok(())
}

/// Resolves when the system asks mctui to go: SIGTERM, SIGHUP (the terminal
/// closed), SIGQUIT or SIGINT; on Windows, the console closing.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut hup), Ok(mut quit), Ok(mut int)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::hangup()),
            signal(SignalKind::quit()),
            signal(SignalKind::interrupt()),
        ) else {
            return std::future::pending().await;
        };
        tokio::select! {
            _ = term.recv() => {}
            _ = hup.recv() => {}
            _ = quit.recv() => {}
            _ = int.recv() => {}
        }
    }
    #[cfg(windows)]
    {
        match tokio::signal::windows::ctrl_close() {
            Ok(mut close) => {
                close.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

/// Reads terminal events on a dedicated thread until reading fails or the app
/// is gone.
fn spawn_input_reader(tx: mpsc::UnboundedSender<Event>) {
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if tx.send(Event::Term(event)).is_err() {
                break;
            }
        }
    });
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
