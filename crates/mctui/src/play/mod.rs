//! Playing: the game session the app owns, and the screen that shows it.
//!
//! A [`Session`] outlives its screen: leave the launch screen and the game
//! keeps running (home shows it playing), come back and the log is all there.
//! The launch runs as app-owned background work reporting [`Msg`]s; the app
//! applies them with [`Session::apply`] and saves what the instance learned.

mod screen;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use jiff::SignedDuration;
use mctui_core::account::{Account, AccountKind};
use mctui_core::auth::{self, Session as AuthSession};
use mctui_core::java::Runtimes;
use mctui_core::launch::{
    self, ExitReport, Game, Identity, InstanceChanges, Launched, Launcher, LogLine, Stage, Step,
};
use mctui_core::modrinth::StarterReport;

pub use screen::{PlayScreen, PlayTarget};

use crate::app::Banner;
use crate::data::Data;
use crate::format;
use crate::msg;
use crate::services::Services;
use crate::task::{Mailbox, Task, Tasks};

/// The most log lines kept; older ones are dropped.
const LOG_CAP: usize = 10_000;
/// Progress updates closer together than this are skipped (the bar can't
/// show them anyway), unless the step moves on.
const STEP_EVERY: Duration = Duration::from_millis(40);
/// A sign-in used for a game must last at least this long.
const SESSION_FOR: SignedDuration = SignedDuration::from_hours(3);

/// Background work reporting to the app.
#[derive(Debug)]
pub enum Msg {
    Step(Step),
    /// The Microsoft sign-in was renewed for this launch.
    Refreshed(Box<Account>),
    Started(Box<Started>),
    Failed(Failure),
    Logs(Vec<LogLine>),
    Exited(ExitReport),
}

/// The game is running: what the launch learned, for the app to keep.
#[derive(Debug)]
pub struct Started {
    pub game: Game,
    pub changes: InstanceChanges,
    /// The version's files came from the cache (offline).
    pub stale: bool,
    pub starter_mods: Option<StarterReport>,
}

/// Why a launch didn't start.
#[derive(Debug)]
pub enum Failure {
    /// The Microsoft sign-in needs doing again.
    SignIn(String),
    Launch(String),
}

/// Where a session is.
#[derive(Debug)]
pub enum Phase {
    /// Getting everything ready; the latest step.
    Preparing(Option<Step>),
    /// Running since `since`.
    Playing {
        game: Game,
        since: Instant,
    },
    /// Asked to stop; waiting for it to save and quit.
    Stopping,
    Ended(ExitReport),
    Failed(Failure),
}

/// One launch of one instance, from Play to the game closing.
#[derive(Debug)]
pub struct Session {
    /// Which launch this is; messages from an earlier one are ignored.
    pub seq: u64,
    pub instance_id: String,
    pub instance_name: String,
    /// "Minecraft 1.21.4 · Fabric 0.16.10".
    pub title: String,
    pub offline: bool,
    /// Who's playing.
    pub player: String,
    pub phase: Phase,
    /// Stages finished so far, in order.
    pub done: Vec<Stage>,
    /// The game's output, oldest first.
    pub log: VecDeque<LogLine>,
    /// Lines dropped because the log was full or the reader fell behind.
    pub dropped: u64,
    /// Worth knowing, but not a failure ("using saved game files").
    pub notes: Vec<String>,
    /// It crashed or failed while the launch screen wasn't showing, and
    /// nobody has looked yet: opening the instance shows why first.
    pub unseen: bool,
    /// The launch, then the log pump. Dropping it cancels a launch in
    /// progress; a running game is unaffected.
    task: Option<Task>,
}

impl Session {
    /// Starts launching `instance_id` in the background. Online launches use
    /// the active Microsoft account; offline ones the active profile's name.
    pub fn start(
        seq: u64,
        instance_id: &str,
        offline: bool,
        data: &Data,
        services: &Services,
        tasks: &Tasks,
    ) -> Option<Self> {
        let instance = data.instance(instance_id)?.clone();
        let account = data.accounts.active().cloned();
        let player = account
            .as_ref()
            .map_or_else(|| "Player".to_owned(), |a| a.name.clone());
        let title = format!(
            "Minecraft {}  ·  {}",
            instance.version,
            loader_label(&instance)
        );
        let instance_name = instance.name.clone();

        let config = data.config.clone();
        let setup = launcher(services, data);
        let auth = services.auth.clone();
        let online = account.filter(|a| !offline && a.kind == AccountKind::Msa);
        let offline_name = player.clone();
        let task = tasks.spawn(move |mail| async move {
            let identity = match online {
                None => Identity::offline(&offline_name),
                Some(account) => match auth
                    .ensure_session(&account, &config.msa_client_id, SESSION_FOR)
                    .await
                {
                    Ok(AuthSession::Valid) => Identity::for_account(&account),
                    Ok(AuthSession::Refreshed(account)) => {
                        let identity = Identity::for_account(&account);
                        mail.send(msg::Msg::Play(seq, Msg::Refreshed(Box::new(account))));
                        identity
                    }
                    Err(e) => {
                        // The old refresh token may be spent: keep the new one.
                        if let Some(account) = e.renewed_account() {
                            let account = Box::new(account.clone());
                            mail.send(msg::Msg::Play(seq, Msg::Refreshed(account)));
                        }
                        return mail.send(msg::Msg::Play(seq, Msg::Failed(sign_in_failure(&e))));
                    }
                },
            };
            let launcher = match setup {
                Ok(launcher) => launcher,
                Err(e) => return mail.send(msg::Msg::Play(seq, Msg::Failed(Failure::Launch(e)))),
            };
            let on_step = throttled(seq, mail.clone());
            match launcher
                .launch(&instance, &config, &identity, on_step)
                .await
            {
                Ok(running) => pump(seq, running, &mail).await,
                Err(e) => mail.send(msg::Msg::Play(
                    seq,
                    Msg::Failed(Failure::Launch(e.to_string())),
                )),
            }
        });
        Some(Self {
            seq,
            instance_id: instance_id.to_owned(),
            instance_name,
            title,
            offline,
            player,
            phase: Phase::Preparing(None),
            done: Vec::new(),
            log: VecDeque::new(),
            dropped: 0,
            notes: Vec::new(),
            unseen: false,
            task: Some(task),
        })
    }

    /// Whether the launch is under way or the game is running.
    pub fn is_live(&self) -> bool {
        matches!(
            self.phase,
            Phase::Preparing(_) | Phase::Playing { .. } | Phase::Stopping
        )
    }

    /// Whether opening `id`'s launch screen shows this session rather than
    /// starting another: it's under way, or it went wrong unseen.
    pub fn holds(&self, id: &str) -> bool {
        self.is_live() || (self.unseen && self.instance_id == id)
    }

    /// Whether it crashed or failed.
    pub fn went_wrong(&self) -> bool {
        match &self.phase {
            Phase::Ended(report) => report.crashed(),
            Phase::Failed(_) => true,
            _ => false,
        }
    }

    /// Whether the game itself is running.
    pub fn is_running(&self) -> bool {
        matches!(self.phase, Phase::Playing { .. } | Phase::Stopping)
    }

    /// When the launch screen next changes on its own: spinner frames while
    /// something's under way, the play timer's next minute while playing.
    pub fn next_deadline(&self, now: Instant) -> Option<Instant> {
        match &self.phase {
            // The spinner is drawn from the wall clock (see `spinner_at`).
            Phase::Preparing(_) | Phase::Stopping => Some(crate::ui::widgets::next_spin_at(
                jiff::Timestamp::now(),
                now,
            )),
            Phase::Playing { since, .. } => {
                let minutes = now.saturating_duration_since(*since).as_secs() / 60 + 1;
                Some(*since + Duration::from_secs(minutes * 60))
            }
            Phase::Ended(_) | Phase::Failed(_) => None,
        }
    }

    /// Asks the game to save and quit (then kills it if it won't).
    pub fn stop(&mut self) {
        if let Phase::Playing { game, .. } = &self.phase {
            game.stop();
            self.phase = Phase::Stopping;
        }
    }

    /// Applies what the background work reported. Returns a toast to show.
    pub fn apply(&mut self, msg: Msg, data: &mut Data) -> Option<Banner> {
        match msg {
            Msg::Step(step) => {
                let stage = step.stage();
                if let Phase::Preparing(Some(prev)) = &self.phase
                    && prev.stage() != stage
                    && !self.done.contains(&prev.stage())
                {
                    self.done.push(prev.stage());
                }
                self.phase = Phase::Preparing(Some(step));
                None
            }
            Msg::Refreshed(account) => data
                .accounts
                .update(&data.dir, |accounts| accounts.upsert(*account))
                .err()
                .map(|e| Banner::warn(format!("Couldn't save your sign-in: {e}"))),
            Msg::Started(started) => {
                let Started {
                    game,
                    changes,
                    stale,
                    starter_mods,
                } = *started;
                if let Phase::Preparing(Some(step)) = &self.phase {
                    self.done.push(step.stage());
                }
                self.phase = Phase::Playing {
                    game,
                    since: Instant::now(),
                };
                if stale {
                    self.notes
                        .push("Offline — using the game files saved last time.".into());
                }
                if let Some(stale_java) = &changes.stale_java {
                    self.notes.push(stale_java.to_string());
                }
                let mut banner = None;
                if starter_mods.as_ref().is_some_and(|r| !r.all_ok()) {
                    banner = Some(Banner::warn(
                        "Some starter mods couldn't be installed; mctui will try again next launch.",
                    ));
                }
                if !changes.is_empty() {
                    banner = self
                        .save_instance(data, |inst| changes.apply(inst))
                        .or(banner);
                }
                banner
            }
            Msg::Failed(failure) => {
                self.phase = Phase::Failed(failure);
                self.task = None;
                None
            }
            Msg::Logs(lines) => {
                self.log.extend(lines);
                let over = self.log.len().saturating_sub(LOG_CAP);
                self.log.drain(..over);
                self.dropped += over as u64;
                None
            }
            Msg::Exited(report) => {
                self.task = None;
                let played = format::play_time(report.ran_for.as_secs());
                let failed = self.save_instance(data, |inst| report.record_play(inst));
                let banner = if report.crashed() {
                    Banner::warn(format!("{} crashed after {played}.", self.instance_name))
                } else {
                    Banner::success(format!("Played {} for {played}.", self.instance_name))
                };
                self.phase = Phase::Ended(report);
                Some(failed.unwrap_or(banner))
            }
        }
    }

    /// Changes this session's instance on disk, keeping whatever another
    /// window saved meanwhile, and the app's copy to match. Returns a warning
    /// if saving failed (the app's copy is changed anyway).
    fn save_instance(
        &self,
        data: &mut Data,
        change: impl Fn(&mut mctui_core::instance::Instance),
    ) -> Option<Banner> {
        let inst = data
            .instances
            .iter_mut()
            .find(|i| i.id == self.instance_id)?;
        match data.store.update(&self.instance_id, &change) {
            Ok(saved) => {
                *inst = saved;
                None
            }
            Err(e) => {
                change(inst);
                Some(Banner::warn(format!(
                    "Couldn't save {}: {e}",
                    self.instance_name
                )))
            }
        }
    }
}

/// Whether `id`'s game is launching or running.
pub fn is_playing(data: &Data, id: &str) -> bool {
    data.game
        .as_ref()
        .is_some_and(|g| g.instance_id == id && g.is_live())
}

/// How `id`'s last launch went wrong ("Crashed"), if nobody has looked yet.
pub fn unseen_trouble(data: &Data, id: &str) -> Option<&'static str> {
    let game = data
        .game
        .as_ref()
        .filter(|g| g.instance_id == id && g.unseen)?;
    Some(match game.phase {
        Phase::Failed(_) => "Couldn't start",
        _ => "Crashed",
    })
}

/// A launcher sharing the app's clients.
fn launcher(services: &Services, data: &Data) -> Result<Launcher, String> {
    let runtimes = Runtimes::system(&data.dir).map_err(|e| e.to_string())?;
    let mut launcher = Launcher::new(services.http.clone(), data.dir.clone(), runtimes);
    launcher.mojang = services.mojang.clone();
    launcher.fabric = services.fabric.clone();
    launcher.modrinth = services.modrinth.clone();
    Ok(launcher)
}

/// Reports steps, skipping progress updates closer together than
/// [`STEP_EVERY`] unless the step moves on.
fn throttled(seq: u64, mail: Mailbox) -> impl Fn(Step) + Send + Sync {
    let last: Mutex<Option<(Instant, launch::Stage)>> = Mutex::new(None);
    move |step| {
        let mut last = last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        let due = last.is_none_or(|(at, stage)| {
            stage != step.stage() || now.duration_since(at) >= STEP_EVERY
        });
        if due {
            *last = Some((now, step.stage()));
            mail.send(msg::Msg::Play(seq, Msg::Step(step)));
        }
    }
}

/// Reports the started game, then forwards its output in batches until it
/// closes, then reports how it ended.
async fn pump(seq: u64, launched: Launched, mail: &Mailbox) {
    let Launched {
        game,
        mut logs,
        changes,
        stale,
        starter_mods,
        ..
    } = launched;
    let started = Started {
        game: game.clone(),
        changes,
        stale,
        starter_mods,
    };
    mail.send(msg::Msg::Play(seq, Msg::Started(Box::new(started))));
    loop {
        let Some(first) = logs.recv().await else {
            break;
        };
        let mut batch = vec![first];
        let until = tokio::time::Instant::now() + Duration::from_millis(30);
        while batch.len() < 1000 {
            match tokio::time::timeout_at(until, logs.recv()).await {
                Ok(Some(line)) => batch.push(line),
                Ok(None) | Err(_) => break,
            }
        }
        mail.send(msg::Msg::Play(seq, Msg::Logs(batch)));
    }
    mail.send(msg::Msg::Play(seq, Msg::Exited(game.wait().await)));
}

fn sign_in_failure(e: &auth::Error) -> Failure {
    match e.cause() {
        auth::Error::NeedsSignIn | auth::Error::SessionExpired => Failure::SignIn(
            "Your Microsoft sign-in has expired. Sign in again to play online.".into(),
        ),
        other => Failure::Launch(format!("Couldn't check your Microsoft sign-in: {other}")),
    }
}

/// "Fabric 0.16.10", or just "Vanilla".
fn loader_label(inst: &mctui_core::instance::Instance) -> String {
    let name = inst.loader.display_name();
    if inst.loader_version.is_empty() || name == "Vanilla" {
        name.to_owned()
    } else {
        format!("{name} {}", inst.loader_version)
    }
}
