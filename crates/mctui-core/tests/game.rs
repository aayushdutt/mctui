//! The running game, with shell scripts standing in for Java: its output
//! read whatever it prints, stopping it, and how it ended.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mctui_core::launch::{Error, Exit, Game, Level, LogLine, Logs, Stopped, Stream};
use tokio::process::Command;

/// Writes `body` as an executable `/bin/sh` script in `dir`.
fn script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("java");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn spawn(dir: &Path, body: &str, grace: Duration) -> (Game, Logs) {
    Game::spawn(Command::new(script(dir, body)), grace).unwrap()
}

async fn all_lines(logs: &mut Logs) -> Vec<LogLine> {
    let mut lines = Vec::new();
    while let Some(line) = logs.recv().await {
        lines.push(line);
    }
    lines
}

/// Waits for the game with a deadline, so a hang fails the test.
async fn wait(game: &Game) -> mctui_core::launch::ExitReport {
    tokio::time::timeout(Duration::from_secs(10), game.wait())
        .await
        .expect("the game should have ended")
}

#[tokio::test]
async fn reads_every_line_whatever_the_game_prints() {
    let tmp = tempfile::tempdir().unwrap();
    // One 200 KB line, invalid UTF-8, then enough to fill a pipe if unread,
    // stderr, and a failing exit.
    let (game, mut logs) = spawn(
        tmp.path(),
        r#"head -c 200000 /dev/zero | tr '\0' 'x'; echo
printf 'caf\351\n'
printf '[12:00:00] [main/WARN]: careful\r\n'
i=0; while [ $i -lt 3000 ]; do echo "line $i"; i=$((i+1)); done
echo 'Exception in thread "main" java.lang.Error: boom' >&2
exit 3"#,
        Duration::from_secs(1),
    );
    let lines = tokio::time::timeout(Duration::from_secs(10), all_lines(&mut logs))
        .await
        .expect("the output should end");
    let report = wait(&game).await;
    assert_eq!(report.exit, Exit::Code(3));
    assert!(report.crashed() && report.stopped.is_none());
    assert_eq!(logs.skipped(), 0, "read as it came");

    let stdout: Vec<&LogLine> = lines
        .iter()
        .filter(|l| l.stream == Stream::Stdout)
        .collect();
    assert_eq!(stdout.len(), 3 + 3000);
    assert_eq!(stdout[0].text.len(), 16 * 1024, "capped");
    assert!(stdout[0].text.bytes().all(|b| b == b'x'));
    assert_eq!(stdout[1].text, "caf\u{fffd}");
    assert_eq!(
        (stdout[2].text.as_str(), stdout[2].level),
        ("[12:00:00] [main/WARN]: careful", Level::Warn)
    );
    assert_eq!(stdout.last().unwrap().text, "line 2999");
    let stderr: Vec<&LogLine> = lines
        .iter()
        .filter(|l| l.stream == Stream::Stderr)
        .collect();
    assert_eq!(stderr.len(), 1);
    assert_eq!(stderr[0].level, Level::Error);
}

#[tokio::test]
async fn log4j_events_arrive_as_log_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let (game, mut logs) = spawn(
        tmp.path(),
        r#"cat <<'EOF'
<log4j:Event logger="net.minecraft.client.Minecraft" timestamp="1700000000000" level="INFO" thread="Render thread">
  <log4j:Message><![CDATA[Setting user: Steve]]></log4j:Message>
</log4j:Event>
<log4j:Event logger="x" timestamp="1700000000000" level="ERROR" thread="Render thread">
  <log4j:Message><![CDATA[Crashed]]></log4j:Message>
</log4j:Event>
EOF"#,
        Duration::from_secs(1),
    );
    let lines = all_lines(&mut logs).await;
    wait(&game).await;
    let levels: Vec<Level> = lines.iter().map(|l| l.level).collect();
    assert_eq!(levels, [Level::Info, Level::Error]);
    assert!(
        lines[0]
            .text
            .ends_with("[Render thread/INFO]: Setting user: Steve"),
        "{:?}",
        lines[0].text
    );
}

#[tokio::test]
async fn stop_interrupts_so_the_game_can_save() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("saved");
    let (game, mut logs) = spawn(
        tmp.path(),
        &format!(
            "trap 'echo saved > \"{}\"; exit 130' INT\necho running\nwhile true; do sleep 0.05; done",
            marker.display()
        ),
        Duration::from_secs(10),
    );
    assert_eq!(logs.recv().await.unwrap().text, "running");
    assert!(game.try_wait().is_none());
    game.stop();
    let report = wait(&game).await;
    assert_eq!(report.stopped, Some(Stopped::Interrupted));
    assert_eq!(report.exit, Exit::Code(130));
    assert!(!report.crashed());
    assert!(marker.exists(), "its handler ran");
    assert_eq!(game.try_wait(), Some(report));
}

#[tokio::test]
async fn a_game_ignoring_the_interrupt_is_killed_after_the_grace_period() {
    let tmp = tempfile::tempdir().unwrap();
    let (game, mut logs) = spawn(
        tmp.path(),
        "trap '' INT\necho running\nwhile true; do sleep 0.05; done",
        Duration::from_millis(300),
    );
    assert_eq!(logs.recv().await.unwrap().text, "running");
    let asked = Instant::now();
    game.stop();
    game.stop(); // Asking twice is harmless.
    let report = wait(&game).await;
    assert_eq!(report.stopped, Some(Stopped::Killed));
    assert_eq!(report.exit, Exit::Signal(9));
    assert!(asked.elapsed() >= Duration::from_millis(300));
    assert!(logs.recv().await.is_none(), "the output ends with it");
}

#[tokio::test]
async fn a_child_holding_the_output_open_does_not_hold_the_game() {
    let tmp = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let (game, mut logs) = spawn(
        tmp.path(),
        "sleep 10 &\necho started",
        Duration::from_secs(1),
    );
    let report = wait(&game).await;
    assert_eq!(report.exit, Exit::Code(0));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(report.ran_for < Duration::from_secs(2));
    let lines = all_lines(&mut logs).await;
    assert_eq!(lines.len(), 1, "the output is closed too");
}

#[tokio::test]
async fn a_slow_reader_misses_the_oldest_lines_but_never_blocks_the_game() {
    let tmp = tempfile::tempdir().unwrap();
    let (game, mut logs) = spawn(
        tmp.path(),
        "i=0; while [ $i -lt 5000 ]; do echo \"line $i\"; i=$((i+1)); done",
        Duration::from_secs(1),
    );
    // Nothing read until the game is done.
    assert_eq!(wait(&game).await.exit, Exit::Code(0));
    let lines = all_lines(&mut logs).await;
    assert_eq!(lines.last().unwrap().text, "line 4999");
    assert_eq!(logs.skipped() + lines.len() as u64, 5000);
    assert!(logs.skipped() > 0);
}

#[tokio::test]
async fn dropping_every_handle_leaves_the_game_running() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("still-playing");
    let (game, logs) = spawn(
        tmp.path(),
        &format!("sleep 0.3\necho yes > \"{}\"", marker.display()),
        Duration::from_secs(1),
    );
    let pid = game.pid().unwrap();
    drop((game, logs));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(marker.exists(), "process {pid} carried on and finished");
}

#[tokio::test]
async fn java_that_cannot_start_is_an_error() {
    let missing = PathBuf::from("/no/such/java");
    let err = Game::spawn(Command::new(&missing), Duration::from_secs(1)).unwrap_err();
    assert!(
        matches!(&err, Error::Spawn { java, .. } if *java == missing),
        "{err}"
    );
    assert!(
        err.to_string()
            .starts_with("couldn't start Java (/no/such/java)"),
        "{err}"
    );
}
