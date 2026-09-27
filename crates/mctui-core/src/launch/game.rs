//! The running game: its output, stopping it, and how it ended.

use std::borrow::Cow;
use std::fmt;
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
use crate::account::Secret;
use crate::instance::Instance;

/// How long a stopped game gets to save and quit before it's killed. Saving
/// a big world can take a while.
pub const DEFAULT_STOP_GRACE: Duration = Duration::from_secs(30);

/// `CreateProcess` flags for the game: a process group of its own, so Ctrl-C
/// in mctui's console doesn't reach it, and a console of its own that isn't
/// shown, so closing mctui's doesn't end it.
#[cfg(windows)]
const DETACHED: u32 = CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
    /// It was asked to quit (interrupted as by Ctrl-C, or on Windows its
    /// window closed), and did, saving on the way.
    Interrupted,
    /// It was killed, with everything it started: it didn't quit within the
    /// grace period, or couldn't be asked to.
    Killed,
}

/// The command that starts the game, for [`Game::spawn`]: from
/// [`Plan::command`](super::Plan::command), or any command. The access
/// token on its command line is hidden from its `Debug` output, and from
/// the game's output once it runs.
pub struct GameCommand {
    command: Command,
    secret: Option<Secret>,
}

impl GameCommand {
    /// `command`, with `secret` to hide wherever it shows.
    pub(super) fn new(command: Command, secret: Option<Secret>) -> Self {
        Self { command, secret }
    }
}

/// A command with no token to hide.
impl From<Command> for GameCommand {
    fn from(command: Command) -> Self {
        Self::new(command, None)
    }
}

impl fmt::Debug for GameCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let command = self.command.as_std();
        let args: Vec<String> = command
            .get_args()
            .map(|arg| redact(&arg.to_string_lossy(), self.secret.as_ref()).into_owned())
            .collect();
        f.debug_struct("GameCommand")
            .field("program", &command.get_program())
            .field("args", &args)
            .field("current_dir", &command.get_current_dir())
            .finish_non_exhaustive()
    }
}

/// `text` with `secret` replaced by `<redacted>` wherever it appears.
pub(super) fn redact<'a>(text: &'a str, secret: Option<&Secret>) -> Cow<'a, str> {
    match secret.map(Secret::expose) {
        Some(token) if !token.is_empty() && text.contains(token) => {
            Cow::Owned(text.replace(token, "<redacted>"))
        }
        _ => Cow::Borrowed(text),
    }
}

impl Game {
    /// Starts `command` as the game, with its output piped to [`Logs`] and no
    /// input. It gets a process group of its own (and on Windows a hidden
    /// console of its own), so it outlives mctui's terminal. [`Game::stop`]
    /// asks it to quit, then kills it and whatever it started if it's still
    /// running after `stop_grace`. Must be called within a tokio runtime.
    pub fn spawn(command: impl Into<GameCommand>, stop_grace: Duration) -> Result<(Game, Logs)> {
        let GameCommand {
            mut command,
            secret,
        } = command.into();
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false);
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(windows)]
        command.creation_flags(DETACHED);
        let mut child = command.spawn().map_err(|source| Error::Spawn {
            java: PathBuf::from(command.as_std().get_program()),
            source,
        })?;
        let (started_at, started) = (Timestamp::now(), Instant::now());

        let (lines, logs) = broadcast::channel(LOG_BUFFER);
        let time_zone = TimeZone::system();
        let mut readers = Vec::with_capacity(2);
        if let Some(stdout) = child.stdout.take() {
            let output = Output {
                stream: Stream::Stdout,
                lines: lines.clone(),
                time_zone: time_zone.clone(),
                secret: secret.clone(),
            };
            readers.push(tokio::spawn(read_output(stdout, output)));
        }
        if let Some(stderr) = child.stderr.take() {
            let output = Output {
                stream: Stream::Stderr,
                lines,
                time_zone,
                secret,
            };
            readers.push(tokio::spawn(read_output(stderr, output)));
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

    /// Asks the game to stop: interrupts it, as Ctrl-C would (on Windows,
    /// closes its window), so it saves and quits, and kills it and whatever
    /// it started if it's still running after the grace period. Returns
    /// straight away; [`Game::wait`] for it.
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
    if let Some(status) = ask_to_quit(child, grace).await {
        return (status.into(), Some(Stopped::Interrupted));
    }
    // Everything the game started goes too, or it could hold the world open.
    #[cfg(unix)]
    let killed = kill_group(child);
    #[cfg(windows)]
    let killed = taskkill(child.id(), true).await;
    #[cfg(not(any(unix, windows)))]
    let killed: io::Result<()> = Err(io::ErrorKind::Unsupported.into());
    if let Err(error) = killed {
        tracing::debug!(%error, "couldn't kill what the game started");
        if let Err(error) = child.start_kill() {
            tracing::warn!(%error, "couldn't kill the game");
        }
    }
    (child.wait().await.into(), Some(Stopped::Killed))
}

/// Asks the game to quit, as Ctrl-C would (on Windows, as closing its
/// window would), and waits up to `grace` for it to. `None` if it couldn't
/// be asked, or is still running.
async fn ask_to_quit(child: &mut Child, grace: Duration) -> Option<io::Result<ExitStatus>> {
    #[cfg(unix)]
    let asked = interrupt(child);
    #[cfg(windows)]
    let asked = taskkill(child.id(), false).await;
    #[cfg(not(any(unix, windows)))]
    let asked: io::Result<()> = Err(io::ErrorKind::Unsupported.into());
    if let Err(error) = asked {
        tracing::debug!(%error, "couldn't ask the game to quit");
        return None;
    }
    tokio::time::timeout(grace, child.wait()).await.ok()
}

/// The game's pid. It hasn't been waited for yet, so the pid can't have
/// been reused.
#[cfg(unix)]
fn pid(child: &Child) -> io::Result<rustix::process::Pid> {
    child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| io::Error::other("the game has already exited"))
}

/// Sends the game SIGINT.
#[cfg(unix)]
fn interrupt(child: &Child) -> io::Result<()> {
    use rustix::process::{Signal, kill_process};
    kill_process(pid(child)?, Signal::INT).map_err(io::Error::from)
}

/// Kills the game's process group: the game, which leads it, and whatever
/// it started.
#[cfg(unix)]
fn kill_group(child: &Child) -> io::Result<()> {
    use rustix::process::{Signal, kill_process_group};
    kill_process_group(pid(child)?, Signal::KILL).map_err(io::Error::from)
}

/// Runs `taskkill` on the game (`pid`, from [`Child::id`]) and whatever it
/// started: asking them to close (their windows get a close request, as
/// from the title bar), or with `force`, ending them. The game's process
/// handle is still open, so its pid can't have been reused.
#[cfg(windows)]
async fn taskkill(pid: Option<u32>, force: bool) -> io::Result<()> {
    let pid = pid
        .ok_or_else(|| io::Error::other("the game has already exited"))?
        .to_string();
    let mut taskkill = Command::new("taskkill");
    taskkill.args(["/PID", pid.as_str(), "/T"]);
    if force {
        taskkill.arg("/F");
    }
    let status = taskkill
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .await?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("taskkill failed ({status})")))
    }
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

/// Where one of the game's outputs goes.
struct Output {
    stream: Stream,
    lines: broadcast::Sender<LogLine>,
    time_zone: TimeZone,
    /// The access token, which old versions print ("Session ID is
    /// token:…"): it's hidden before a line goes anywhere.
    secret: Option<Secret>,
}

/// Reads one of the game's outputs to the end, sending each line on.
/// Keeps reading whether anyone listens or not, so the game never blocks on
/// a full pipe.
async fn read_output(output: impl AsyncRead + Unpin, to: Output) {
    let Output {
        stream,
        lines,
        time_zone,
        secret,
    } = to;
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
        let line = text(&raw);
        parser.push(&redact(&line, secret.as_ref()), |text, level| {
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

    #[tokio::test]
    async fn the_access_token_never_reaches_the_logs() {
        let (lines, mut logs) = broadcast::channel(16);
        let output = Output {
            stream: Stream::Stdout,
            lines,
            time_zone: TimeZone::UTC,
            secret: Some(Secret::new("tok-123")),
        };
        // What 1.7.x prints.
        let printed = b"(Session ID is token:tok-123:069a79f4)\nSetting user: Steve\n";
        read_output(&printed[..], output).await;
        let first = logs.recv().await.unwrap();
        assert_eq!(first.text, "(Session ID is token:<redacted>:069a79f4)");
        assert_eq!(logs.recv().await.unwrap().text, "Setting user: Steve");
    }

    #[test]
    fn game_commands_hide_the_token_from_debug() {
        let mut command = Command::new("java");
        command.args(["--accessToken", "tok-123", "--session", "token:tok-123:u"]);
        let secret = Some(Secret::new("tok-123"));
        let debug = format!("{:?}", GameCommand::new(command, secret));
        assert!(!debug.contains("tok-123"), "{debug}");
        assert!(debug.contains("token:<redacted>:u"), "{debug}");

        assert_eq!(
            redact("a tok b", Some(&Secret::new("tok"))),
            "a <redacted> b"
        );
        assert_eq!(redact("same", Some(&Secret::new(""))), "same");
        assert_eq!(redact("same", None), "same");
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
