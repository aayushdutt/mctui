//! The launch screen and the game session, driven through the whole app.

use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::KeyCode;
use mctui_core::launch::{Exit, ExitReport, Game, InstanceChanges, Level, LogLine, Step, Stream};
use mctui_core::net::Progress;

use super::*;
use crate::app::tests::Harness;

const ID: &str = "Survival 1.21";

/// A session for the fixture's first instance, in `phase`.
fn session(seq: u64, phase: Phase) -> Session {
    Session {
        seq,
        instance_id: ID.into(),
        instance_name: ID.into(),
        title: "Minecraft 1.21.4  ·  Fabric 0.16.10".into(),
        offline: false,
        player: "Steve".into(),
        phase,
        done: Vec::new(),
        log: VecDeque::new(),
        dropped: 0,
        missed: 0,
        notes: Vec::new(),
        unseen: false,
        task: None,
    }
}

fn line(text: &str, level: Level) -> LogLine {
    LogLine {
        text: text.into(),
        level,
        stream: Stream::Stdout,
    }
}

/// A harness on the launch screen of a session in `phase`.
fn on_play_screen(phase: Phase) -> Harness {
    let mut h = Harness::new(100, 30);
    h.app.sessions_for_tests(1);
    h.app.data_mut().game = Some(session(1, phase));
    h.app.show_play_screen_for_tests(ID);
    h.draw();
    h
}

fn playing(game: Game) -> Phase {
    Phase::Playing {
        game,
        since: std::time::Instant::now(),
    }
}

/// A real process standing in for the game: it runs until stopped.
fn fake_game(h: &Harness) -> Game {
    let mut command = tokio::process::Command::new("sh");
    command.args(["-c", "sleep 30"]);
    h.block_on(async { Game::spawn(command, Duration::from_millis(200)) })
        .expect("spawn")
        .0
}

fn exit(code: i32) -> ExitReport {
    ExitReport {
        exit: Exit::Code(code),
        stopped: None,
        started_at: crate::testing::now(),
        ran_for: Duration::from_secs(62 * 60),
    }
}

#[test]
fn preparing_shows_the_steps() {
    let step = Step::Libraries(Progress {
        files_done: 40,
        files_total: 120,
        bytes_done: 9_000_000,
        bytes_total: 24_000_000,
    });
    let mut h = on_play_screen(Phase::Preparing(Some(step)));
    if let Some(game) = &mut h.app.data_mut().game {
        game.done = vec![Stage::Version, Stage::Java];
    }
    insta::assert_snapshot!(h.text());
}

#[test]
fn playing_shows_the_log_filtered() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = on_play_screen(playing(game));
    if let Some(session) = &mut h.app.data_mut().game {
        session.log.extend([
            line(
                "[12:00:01] [Render thread/INFO]: Setting user: Steve",
                Level::Info,
            ),
            line(
                "[12:00:02] [Render thread/WARN]: Missing sound for event",
                Level::Warn,
            ),
            line(
                "[12:00:03] [Worker/ERROR]: Couldn't load a texture",
                Level::Error,
            ),
        ]);
    }
    let errors_only = h.text();
    assert!(
        errors_only.contains("Couldn't load a texture") && !errors_only.contains("Setting user")
    );
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('v'));
    insta::assert_snapshot!(h.text());
    assert_eq!(
        h.data().config.launch_log_verbosity,
        mctui_core::config::LogVerbosity::All,
        "saved"
    );
}

#[test]
fn a_crash_stays_to_show_why_and_offers_another_go() {
    let mut h = on_play_screen(Phase::Ended(exit(1)));
    if let Some(session) = &mut h.app.data_mut().game {
        session.log.push_back(line(
            "java.lang.OutOfMemoryError: Java heap space",
            Level::Error,
        ));
    }
    insta::assert_snapshot!(h.text());
}

#[test]
fn a_failed_launch_offers_retry_and_offline() {
    let failure =
        Failure::Launch("Couldn't reach piston-meta.mojang.com — check your connection.".into());
    let mut h = on_play_screen(Phase::Failed(failure));
    insta::assert_snapshot!(h.text());
}

#[test]
fn messages_advance_the_session_and_save_the_instance() {
    let mut h = Harness::new(100, 30);
    h.app.sessions_for_tests(1);
    h.app.data_mut().game = Some(session(1, Phase::Preparing(None)));
    let send = |h: &mut Harness, msg| {
        h.app.handle(crate::task::Event::Msg(
            u64::MAX,
            crate::msg::Msg::Play(1, msg),
        ));
    };
    send(&mut h, Msg::Step(Step::Version));
    send(&mut h, Msg::Step(Step::Java));
    let game = fake_game(&h);
    let changes = InstanceChanges {
        loader_version: None,
        downloaded: Some(
            h.data()
                .instance(ID)
                .map(mctui_core::instance::Instance::download_key)
                .unwrap_or_default(),
        ),
        starter_mods_installed: false,
        stale_java: None,
    };
    send(
        &mut h,
        Msg::Started(Box::new(Started {
            game: game.clone(),
            changes,
            stale: false,
            starter_mods: None,
        })),
    );
    let session = h.data().game.as_ref().unwrap();
    assert!(session.is_running());
    assert_eq!(session.done, [Stage::Version, Stage::Java]);
    assert!(super::is_playing(h.data(), ID));

    // A message from an earlier launch is ignored.
    h.app.handle(crate::task::Event::Msg(
        u64::MAX,
        crate::msg::Msg::Play(0, Msg::Exited(exit(0))),
    ));
    assert!(h.data().game.as_ref().unwrap().is_running());

    let before = h.data().instance(ID).unwrap().play_time_secs;
    send(&mut h, Msg::Exited(exit(0)));
    game.stop();
    let inst = h.data().instance(ID).unwrap();
    assert_eq!(inst.play_time_secs, before + 62 * 60);
    assert!(inst.is_fully_downloaded);
    let saved = h.data().store.scan().unwrap().instances;
    assert!(
        saved
            .iter()
            .any(|i| i.id == ID && i.play_time_secs == before + 62 * 60),
        "saved to disk"
    );
    assert_eq!(h.banner(), Some("Played Survival 1.21 for 1h 2m."));
}

/// Another mctui window renamed the instance while this one played: the
/// play time is added to the instance as it is on disk, keeping the rename.
#[test]
fn play_time_is_saved_without_losing_another_windows_change() {
    let mut h = Harness::new(100, 30);
    h.app.sessions_for_tests(1);
    h.app.data_mut().game = Some(session(1, Phase::Preparing(None)));
    let before = h.data().instance(ID).unwrap().play_time_secs;
    h.data()
        .store
        .update(ID, |inst| inst.name = "Renamed elsewhere".into())
        .expect("the other window saves");

    h.app.handle(crate::task::Event::Msg(
        u64::MAX,
        crate::msg::Msg::Play(1, Msg::Exited(exit(0))),
    ));

    let saved = h.data().store.scan().unwrap().instances;
    let on_disk = saved.iter().find(|i| i.id == ID).unwrap();
    assert_eq!(on_disk.name, "Renamed elsewhere");
    assert_eq!(on_disk.play_time_secs, before + 62 * 60);
    assert_eq!(h.data().instance(ID), Some(on_disk), "the app's copy too");
}

#[test]
fn esc_while_preparing_cancels() {
    let mut h = on_play_screen(Phase::Preparing(Some(Step::Version)));
    h.press(KeyCode::Esc);
    assert!(h.data().game.is_none());
    assert_eq!(h.banner(), Some("Launch cancelled."));
    assert!(h.text().contains("INSTANCES"), "back home");
}

#[test]
fn quitting_while_playing_asks_twice() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = on_play_screen(playing(game.clone()));
    h.press(KeyCode::Esc);
    assert!(h.text().contains("Playing"), "home shows the game running");
    h.press(KeyCode::Char('q'));
    assert!(!h.app.should_quit());
    assert!(h.banner().is_some_and(|b| b.contains("still running")));
    h.press(KeyCode::Char('q'));
    assert!(h.app.should_quit());
    game.stop();
}

#[test]
fn another_instance_cant_start_while_one_runs() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = on_play_screen(playing(game.clone()));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Down);
    h.press(KeyCode::Char('o'));
    assert!(h.banner().is_some_and(|b| b.contains("still running")));
    game.stop();
}

#[test]
fn offline_play_with_no_services_fails_gracefully() {
    let mut h = Harness::new(100, 30);
    h.press(KeyCode::Char('o'));
    // Retries against the unreachable services take a couple of seconds.
    for _ in 0..50 {
        h.settle();
        if !h.data().game.as_ref().is_some_and(Session::is_live) {
            break;
        }
    }
    let text = h.text();
    assert!(text.contains("Couldn't launch Minecraft"), "{text}");
    assert!(
        h.data()
            .game
            .as_ref()
            .is_some_and(|g| matches!(g.phase, Phase::Failed(_)))
    );
}

#[test]
fn the_running_instance_cant_be_edited_or_deleted() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = on_play_screen(playing(game.clone()));
    h.press(KeyCode::Esc);
    for key in ['e', 'm', 'p'] {
        h.press(KeyCode::Char(key));
        assert_eq!(
            h.banner(),
            Some("Close Minecraft before changing this instance."),
            "{key}"
        );
        assert!(h.text().contains("INSTANCES"), "still home after {key}");
    }
    h.press(KeyCode::Char('d'));
    assert_eq!(
        h.banner(),
        Some("Close Minecraft before deleting this instance.")
    );
    game.stop();
}

#[test]
fn view_game_never_detours_through_sign_in() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = on_play_screen(playing(game.clone()));
    h.app.data_mut().accounts = mctui_core::account::Accounts::default();
    h.press(KeyCode::Esc);
    h.press(KeyCode::Enter);
    assert!(h.text().contains("GAME LOG"), "{}", h.text());
    game.stop();
}

#[test]
fn a_game_that_starts_after_its_launch_was_cancelled_is_stopped() {
    let mut h = Harness::new(100, 30);
    let game = fake_game(&h);
    let started = Started {
        game: game.clone(),
        changes: InstanceChanges::default(),
        stale: false,
        starter_mods: None,
    };
    h.app.handle(crate::task::Event::Msg(
        u64::MAX,
        crate::msg::Msg::Play(7, Msg::Started(Box::new(started))),
    ));
    let ended =
        h.block_on(async { tokio::time::timeout(Duration::from_secs(5), game.wait()).await });
    assert!(ended.is_ok(), "the orphaned game was stopped");
}

#[test]
fn the_second_q_quits_from_any_screen() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = on_play_screen(playing(game.clone()));
    h.press(KeyCode::Char(':'));
    h.press(KeyCode::Esc);
    h.app
        .handle(crate::task::Event::Term(crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new(
                KeyCode::Char('c'),
                crossterm::event::KeyModifiers::CONTROL,
            ),
        )));
    assert!(!h.app.should_quit());
    h.press(KeyCode::Char('q'));
    assert!(h.app.should_quit(), "q on the launch screen confirms");
    game.stop();
}

#[test]
fn the_log_scrolls_to_a_full_first_page_and_says_what_it_dropped() {
    let mut h = on_play_screen(Phase::Ended(exit(0)));
    if let Some(session) = &mut h.app.data_mut().game {
        session
            .log
            .extend((1..=200).map(|i| line(&format!("line {i}"), Level::Error)));
        session.dropped = 1_800;
    }
    let text = h.text();
    assert!(
        text.contains("line 200") && !text.contains("weren't kept"),
        "{text}"
    );

    h.press(KeyCode::Char('g'));
    let text = h.text();
    assert!(
        text.contains("… 1,800 lines weren't kept") && text.contains("line 1\n"),
        "{text}"
    );
    let shown = text.lines().filter(|l| l.contains("line ")).count();
    assert!(shown > 5, "the first page is full, not one line: {text}");
    h.press(KeyCode::Char('j'));
    assert!(
        !h.text().contains("line 1\n"),
        "one step down moves the view at once"
    );
}

#[test]
fn a_crash_while_away_waits_on_home_until_seen() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = Harness::new(100, 30);
    h.app.sessions_for_tests(1);
    let mut crashed = session(1, playing(game.clone()));
    crashed.log.push_back(line(
        "java.lang.OutOfMemoryError: Java heap space",
        Level::Error,
    ));
    h.app.data_mut().game = Some(crashed);
    h.app.handle(crate::task::Event::Msg(
        u64::MAX,
        crate::msg::Msg::Play(1, Msg::Exited(exit(1))),
    ));
    game.stop();
    let text = h.text();
    assert!(text.contains("▲ Crashed"), "{text}");
    assert!(text.contains("see what happened"), "{text}");

    h.press(KeyCode::Enter);
    let text = h.text();
    assert!(text.contains("OutOfMemoryError"), "shows why: {text}");
    assert_eq!(
        h.data().game.as_ref().map(|g| g.seq),
        Some(1),
        "not relaunched"
    );

    h.press(KeyCode::Esc);
    let text = h.text();
    assert!(
        !text.contains("Crashed") && text.contains("↵  play"),
        "{text}"
    );
}

#[test]
fn a_sign_in_renewed_by_a_launch_settles_the_accounts_check() {
    use crate::accounts::SessionStatus;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    let mut h = Harness::online(100, 30, |_| {});
    // Minecraft is slow to answer the startup check, then turns it down.
    h.mock(
        Mock::given(method("GET"))
            .and(path("/minecraft/profile"))
            .respond_with(ResponseTemplate::new(401).set_delay(Duration::from_millis(300))),
    );
    h.app.start();
    let account = h.data().accounts.active().cloned().unwrap();
    assert_eq!(
        h.data().sessions.get(&account.id),
        Some(&SessionStatus::Checking)
    );

    h.app.sessions_for_tests(1);
    h.app.data_mut().game = Some(session(1, Phase::Preparing(None)));
    let id = account.id.clone();
    h.app.handle(crate::task::Event::Msg(
        u64::MAX,
        crate::msg::Msg::Play(1, Msg::Refreshed(Box::new(account))),
    ));
    std::thread::sleep(Duration::from_millis(400));
    h.settle();
    assert_eq!(h.data().sessions.get(&id), Some(&SessionStatus::Valid));
}

#[test]
fn ctrl_z_doesnt_suspend_while_the_game_runs() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = on_play_screen(playing(game.clone()));
    let key = crossterm::event::KeyEvent::new(
        KeyCode::Char('z'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    h.app
        .handle(crate::task::Event::Term(crossterm::event::Event::Key(key)));
    assert!(!h.app.take_suspend());
    if cfg!(unix) {
        assert!(h.banner().is_some_and(|b| b.contains("can't pause")));
    }
    game.stop();
}

#[test]
fn a_scrolled_back_log_stays_put_as_lines_arrive() {
    let h = Harness::new(100, 30);
    let game = fake_game(&h);
    drop(h);
    let mut h = on_play_screen(playing(game.clone()));
    if let Some(session) = &mut h.app.data_mut().game {
        session
            .log
            .extend((1..=200).map(|i| line(&format!("line {i}"), Level::Error)));
    }
    h.draw();
    for _ in 0..5 {
        h.press(KeyCode::Char('k'));
    }
    let view = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| l.trim_start().starts_with("line "))
            .map(|l| l.trim().to_owned())
            .collect()
    };
    let before = view(&h.text());
    assert_eq!(before.last().map(String::as_str), Some("line 195"));

    let more = (201..=250)
        .map(|i| line(&format!("line {i}"), Level::Error))
        .collect();
    h.app.handle(crate::task::Event::Msg(
        u64::MAX,
        crate::msg::Msg::Play(1, Msg::Logs(more)),
    ));
    let text = h.text();
    assert_eq!(view(&text), before, "the same lines");
    assert!(text.contains("↓ 55 newer"), "{text}");
    h.press(KeyCode::Char('G'));
    assert_eq!(view(&h.text()).last().map(String::as_str), Some("line 250"));
    game.stop();
}
