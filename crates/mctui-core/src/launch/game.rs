//! The running game: its output, stopping it, and how it ended.

use std::io;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

use super::log::{LogLine, Parser, Stream};
use super::{Error, Result};
use crate::instance::Instance;

/// How long a stopped game gets to save and quit before it's killed.
pub const DEFAULT_STOP_GRACE: Duration = Duration::from_secs(15);

/// How long output is still read after the game exits. A process the game
/// started may hold its output open for good; the Go app waited for it.
const OUTPUT_DRAIN: Duration = Duration::from_secs(2);

/// Lines held for a reader that falls behind; older ones are dropped.
const LOG_BUFFER: usize = 2048;

/// The most of one line kept; the rest of a longer line is dropped.
const MAX_LINE: usize = 16 * 1024;

/// A running game. Cheap to clone; every clone controls the same game.
///
/// Dropping every handle leaves the game running (the player may quit mctui
/// and keep playing): it only stops when asked to with [`Game::stop`].
#[derive(Debug, Clone)]
pub struct Game {
    pid: Option<u32>,
    started_at: Timestamp,
    stop: Arc<watch::Sender<bool>>,
    exit: watch::Receiver<Option<ExitReport>>,
}

/// The game's output, line by line, from [`Game::spawn`].
///
/// The game's output is always read as it comes, so the game never waits on
/// it. The latest lines are held here until read; a reader that falls
/// behind misses the oldest (see [`Logs::skipped`]).
#[derive(Debug)]
pub struct Logs {
    lines: broadcast::Receiver<LogLine>,
    skipped: u64,
}

/// How a game ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitReport {
    /// How the process ended.
    pub exit: Exit,
    /// Whether it was stopped, and how.
    pub stopped: Option<Stopped>,
    /// When it started.
    pub started_at: Timestamp,
    /// How long it ran.
    pub ran_for: Duration,
}

/// How the game process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// It exited with this code; 0 is a clean quit.
    Code(i32),
    /// A signal ended it (on Unix), e.g. 9 when killed.
    Signal(i32),
    /// Waiting for it failed, so how it ended isn't known.
    Unknown,
}

/// How [`Game::stop`] stopped the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// It was interrupted (as by Ctrl-C), and quit, saving on the way.
    Interrupted,
    /// It was killed: it didn't quit within the grace period, or (on
    /// Windows) couldn't be interrupted.
    Killed,
}

impl Game {
    /// Starts `command` as the game, with its output piped to [`Logs`] and no
    /// input. On Unix it gets a process group of its own, so it outlives
    /// mctui's terminal. [`Game::stop`] interrupts it, then kills it if it's
    /// still running after `stop_grace`. Must be called within a tokio
    /// runtime.
    pub fn spawn(mut command: Command, stop_grace: Duration) -> Result<(Game, Logs)> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn().map_err(|source| Error::Spawn {
            java: PathBuf::from(command.as_std().get_program()),
            source,
        })?;
        let (started_at, started) = (Timestamp::now(), Instant::now());

        let (lines, logs) = broadcast::channel(LOG_BUFFER);
        let time_zone = TimeZone::system();
        let mut readers = Vec::with_capacity(2);
        if let Some(stdout) = child.stdout.take() {
            let reader = read_output(stdout, Stream::Stdout, lines.clone(), time_zone.clone());
            readers.push(tokio::spawn(reader));
        }
        if let Some(stderr) = child.stderr.take() {
            readers.push(tokio::spawn(read_output(
                stderr,
                Stream::Stderr,
                lines,
                time_zone,
            )));
        }

        let pid = child.id();
        let (stop, stop_requested) = watch::channel(false);
        let (exited, exit) = watch::channel(None);
        tokio::spawn(async move {
            let (exit, stopped) = supervise(&mut child, stop_requested, stop_grace).await;
            let ran_for = started.elapsed();
            finish_reading(readers).await;
            exited.send_replace(Some(ExitReport {
                exit,
                stopped,
                started_at,
                ran_for,
            }));
        });

        let game = Game {
            pid,
            started_at,
            stop: Arc::new(stop),
            exit,
        };
        let logs = Logs {
            lines: logs,
            skipped: 0,
        };
        Ok((game, logs))
    }

    /// The game's process id.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// When the game started.
    pub fn started_at(&self) -> Timestamp {
        self.started_at
    }

    /// Asks the game to stop: interrupts it, as Ctrl-C would, so it saves
    /// and quits, and kills it if it's still running after the grace period
    /// (at once on Windows). Returns straight away; [`Game::wait`] for it.
    pub fn stop(&self) {
        self.stop.send_replace(true);
    }

    /// Waits for the game to end and its output to be read.
    pub async fn wait(&self) -> ExitReport {
        let mut exit = self.exit.clone();
        let report = exit
            .wait_for(Option::is_some)
            .await
            .ok()
            .and_then(|r| r.clone());
        // Only if the runtime shut down first.
        report.unwrap_or_else(|| ExitReport {
            exit: Exit::Unknown,
            stopped: None,
            started_at: self.started_at,
            ran_for: Timestamp::now()
                .duration_since(self.started_at)
                .unsigned_abs(),
        })
    }

    /// How the game ended, if it has.
    pub fn try_wait(&self) -> Option<ExitReport> {
        self.exit.borrow().clone()
    }
}

impl Logs {
    /// The next line, waiting for it; `None` once the game has ended and
    /// every line has been read.
    pub async fn recv(&mut self) -> Option<LogLine> {
        loop {
            match self.lines.recv().await {
                Ok(line) => return Some(line),
                Err(broadcast::error::RecvError::Lagged(missed)) => self.skipped += missed,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// How many lines were dropped so far because they weren't read in time.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }
}

impl ExitReport {
    /// Whether the game crashed: it ended badly on its own, not because it
    /// was stopped.
    pub fn crashed(&self) -> bool {
        self.stopped.is_none() && self.exit != Exit::Code(0)
    }

    /// Records this session on `instance`: adds its time to the play time,
    /// and makes its start the last time played. For the caller to save.
    pub fn record_play(&self, instance: &mut Instance) {
        instance.play_time_secs = instance
            .play_time_secs
            .saturating_add(self.ran_for.as_secs());
        instance.last_played = instance.last_played.max(Some(self.started_at));
    }
}

impl From<io::Result<ExitStatus>> for Exit {
    fn from(status: io::Result<ExitStatus>) -> Self {
        let Ok(status) = status else {
            return Exit::Unknown;
        };
        #[cfg(unix)]
        if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
            return Exit::Signal(signal);
        }
        status.code().map_or(Exit::Unknown, Exit::Code)
    }
}

/// Waits for the game to exit, stopping it if asked.
async fn supervise(
    child: &mut Child,
    mut stop_requested: watch::Receiver<bool>,
    grace: Duration,
) -> (Exit, Option<Stopped>) {
    let requested = async {
        if stop_requested.wait_for(|&stop| stop).await.is_err() {
            // Every handle is gone, so no one can ask: let it play on.
            std::future::pending::<()>().await;
        }
    };
    let exited = tokio::select! {
        status = child.wait() => Some(status),
        () = requested => None,
    };
    if let Some(status) = exited {
        return (status.into(), None);
    }
    if interrupt(child).is_ok()
        && let Ok(status) = tokio::time::timeout(grace, child.wait()).await
    {
        return (status.into(), Some(Stopped::Interrupted));
    }
    if let Err(error) = child.start_kill() {
        tracing::warn!(%error, "couldn't kill the game");
    }
    (child.wait().await.into(), Some(Stopped::Killed))
}

/// Sends the game SIGINT. It hasn't been waited for yet, so its pid can't
/// have been reused.
#[cfg(unix)]
fn interrupt(child: &Child) -> io::Result<()> {
    use rustix::process::{Pid, Signal, kill_process};
    let pid = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(Pid::from_raw)
        .ok_or_else(|| io::Error::other("the game has already exited"))?;
    kill_process(pid, Signal::INT).map_err(io::Error::from)
}

/// Windows can't interrupt another process.
#[cfg(not(unix))]
fn interrupt(_child: &Child) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

/// Waits a little for the output readers to reach the end, then stops them.
async fn finish_reading(mut readers: Vec<JoinHandle<()>>) {
    let all_read = async {
        for reader in &mut readers {
            let _ = reader.await;
        }
    };
    if tokio::time::timeout(OUTPUT_DRAIN, all_read).await.is_err() {
        tracing::debug!("the game's output is still open after it exited; closing it");
        for reader in &readers {
            reader.abort();
        }
    }
}

/// Reads one of the game's outputs to the end, sending each line to `lines`.
/// Keeps reading whether anyone listens or not, so the game never blocks on
/// a full pipe.
async fn read_output(
    output: impl AsyncRead + Unpin,
    stream: Stream,
    lines: broadcast::Sender<LogLine>,
    time_zone: TimeZone,
) {
    let mut output = BufReader::with_capacity(64 * 1024, output);
    let mut parser = Parser::new(time_zone);
    let mut raw = Vec::new();
    loop {
        match read_line(&mut output, &mut raw).await {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => {
                tracing::debug!(%error, ?stream, "stopped reading the game's output");
                break;
            }
        }
        parser.push(&text(&raw), |text, level| {
            // No one listening is fine.
            let _ = lines.send(LogLine {
                text,
                level,
                stream,
            });
        });
    }
}

/// Reads one line into `line`, without its line ending, keeping at most
/// [`MAX_LINE`] bytes of it. `false` at the end of the output.
async fn read_line(
    output: &mut (impl AsyncBufRead + Unpin),
    line: &mut Vec<u8>,
) -> io::Result<bool> {
    line.clear();
    let mut read_any = false;
    loop {
        let buf = output.fill_buf().await?;
        if buf.is_empty() {
            break;
        }
        read_any = true;
        let (chunk, used, ended) = match buf.iter().position(|&b| b == b'\n') {
            Some(end) => (&buf[..end], end + 1, true),
            None => (buf, buf.len(), false),
        };
        let room = MAX_LINE.saturating_sub(line.len());
        line.extend_from_slice(&chunk[..chunk.len().min(room)]);
        output.consume(used);
        if ended {
            break;
        }
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(read_any)
}

/// A raw line as text: invalid UTF-8 replaced, and no longer than
/// [`MAX_LINE`] bytes even then.
fn text(raw: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(raw).into_owned();
    text.truncate(text.floor_char_boundary(MAX_LINE));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn lines(input: &[u8]) -> Vec<Vec<u8>> {
        let mut output = BufReader::with_capacity(16, input);
        let mut line = Vec::new();
        let mut lines = Vec::new();
        while read_line(&mut output, &mut line).await.unwrap() {
            lines.push(line.clone());
        }
        lines
    }

    #[tokio::test]
    async fn lines_are_split_and_capped() {
        let long = vec![b'x'; MAX_LINE + 100];
        let mut input = long.clone();
        input.extend_from_slice(b"\nnext\r\n\nlast");
        let got = lines(&input).await;
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], long[..MAX_LINE]);
        assert_eq!(got[1..], [b"next".to_vec(), Vec::new(), b"last".to_vec()]);
    }

    #[test]
    fn text_is_lossy_and_capped() {
        assert_eq!(text(b"caf\xe9"), "caf\u{fffd}");
        // Each bad byte becomes three: cut back to the cap, on a boundary.
        let bad = vec![0xff; MAX_LINE];
        let text = text(&bad);
        assert!(text.len() <= MAX_LINE && text.chars().all(|c| c == '\u{fffd}'));
    }

    #[test]
    fn play_time_adds_up() {
        let mut instance = Instance {
            play_time_secs: 100,
            ..Instance::default()
        };
        let started_at: Timestamp = "2026-09-27T12:00:00Z".parse().unwrap();
        let report = ExitReport {
            exit: Exit::Code(0),
            stopped: None,
            started_at,
            ran_for: Duration::from_millis(61_500),
        };
        report.record_play(&mut instance);
        assert_eq!(instance.play_time_secs, 161);
        assert_eq!(instance.last_played, Some(started_at));
        // An older session doesn't move last played back.
        let earlier = ExitReport {
            started_at: "2026-09-26T12:00:00Z".parse().unwrap(),
            ..report
        };
        earlier.record_play(&mut instance);
        assert_eq!(instance.last_played, Some(started_at));
    }

    #[test]
    fn crashes_are_unasked_bad_exits() {
        let report = |exit, stopped| ExitReport {
            exit,
            stopped,
            started_at: Timestamp::UNIX_EPOCH,
            ran_for: Duration::ZERO,
        };
        assert!(!report(Exit::Code(0), None).crashed());
        assert!(report(Exit::Code(1), None).crashed());
        assert!(report(Exit::Signal(11), None).crashed());
        assert!(!report(Exit::Code(130), Some(Stopped::Interrupted)).crashed());
        assert!(!report(Exit::Signal(9), Some(Stopped::Killed)).crashed());
    }
}
