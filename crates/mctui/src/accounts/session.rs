//! The app's side of accounts: applying changes, saving them, and checking
//! the active Microsoft account's session in the background, at startup and
//! whenever another account becomes active. At most one check runs per
//! account (A-11); a refreshed account replaces the saved one.

use std::collections::HashMap;

use jiff::SignedDuration;
use mctui_core::account::{AccountKind, SaveError};
use mctui_core::auth::{self, Session};

use super::{Change, Msg, offline};
use crate::app::Banner;
use crate::data::Data;
use crate::msg;
use crate::services::Services;
use crate::task::{Task, Tasks};

/// A checked session has to last at least this long to count as working.
const VALID_FOR: SignedDuration = SignedDuration::from_mins(5);

/// What the app knows about a Microsoft account's session this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Checking,
    /// Minecraft accepted it, or it was just signed in or refreshed.
    Valid,
    /// The saved sign-in is gone: sign in again.
    Expired,
    /// Couldn't check: no connection, or a service had a problem. It may
    /// well still work.
    Unverified,
}

/// Session checks in flight, owned by the app.
#[derive(Debug, Default)]
pub struct Checks {
    /// The check running for each account, by id, with its number: a result
    /// from a check that was dropped or replaced is ignored.
    running: HashMap<String, (u64, Task)>,
    last: u64,
}

impl Checks {
    /// Checks the active Microsoft account's session in the background,
    /// unless a check of it is already running.
    pub fn check_active(&mut self, data: &mut Data, tasks: &Tasks, services: &Services) {
        let Some(account) = data
            .accounts
            .active()
            .filter(|a| a.kind == AccountKind::Msa && !self.running.contains_key(&a.id))
        else {
            return;
        };
        self.last += 1;
        let (seq, id) = (self.last, account.id.clone());
        let check = {
            let (auth, account) = (services.auth.clone(), account.clone());
            let client_id = data.config.msa_client_id.clone();
            async move { auth.ensure_session(&account, &client_id, VALID_FOR).await }
        };
        let reply_to = id.clone();
        let task = tasks.run(check, move |result| {
            msg::Msg::Accounts(Msg::Session {
                id: reply_to,
                seq,
                result,
            })
        });
        self.running.insert(id.clone(), (seq, task));
        data.sessions.insert(id, SessionStatus::Checking);
    }

    /// Applies `change`, saves, and checks the session of a newly active
    /// account. Returns the toast that says how it went.
    pub fn apply(
        &mut self,
        change: Change,
        data: &mut Data,
        tasks: &Tasks,
        services: &Services,
    ) -> Option<Banner> {
        let active = data.accounts.active_id.clone();
        // A fresh sign-in needs no check.
        let check = !matches!(change, Change::SignedIn(_));
        let accounts = &mut data.accounts;
        let done = match change {
            Change::Activate(id) => {
                let name = accounts.accounts.iter().find(|a| a.id == id)?.name.clone();
                accounts.set_active(&id);
                format!("Now playing as {name}.")
            }
            Change::Remove(id) => {
                let removed = accounts.remove(&id)?;
                self.running.remove(&id);
                data.sessions.remove(&id);
                if accounts.active_id.is_none()
                    && let Some(next) = accounts.accounts.first()
                {
                    accounts.active_id = Some(next.id.clone());
                }
                format!("Removed “{}”.", removed.name)
            }
            Change::AddOffline(name) => {
                accounts.upsert(offline::account(&name));
                format!("Added the offline profile “{name}”.")
            }
            Change::SignedIn(account) => {
                let (id, name) = (account.id.clone(), account.name.clone());
                // A-3: signing in switches to that account.
                accounts.upsert(*account);
                accounts.set_active(&id);
                // Any check of the old token is moot now.
                self.running.remove(&id);
                data.sessions.insert(id, SessionStatus::Valid);
                format!("Signed in as {name}.")
            }
        };
        let saved = data.accounts.save(&data.dir);
        if check && data.accounts.active_id != active {
            self.check_active(data, tasks, services);
        }
        Some(match saved {
            Ok(()) => Banner::success(done),
            Err(e) => Banner::warn(format!("{done} {}", not_saved(&e))),
        })
    }

    /// A check reported back. Returns a toast if saving a refreshed account
    /// failed.
    pub fn handle(&mut self, msg: Msg, data: &mut Data) -> Option<Banner> {
        let Msg::Session { id, seq, result } = msg else {
            return None;
        };
        if self
            .running
            .get(&id)
            .is_none_or(|(running, _)| *running != seq)
        {
            return None;
        }
        self.running.remove(&id);
        let mut banner = None;
        let status = match result {
            Ok(Session::Valid) => SessionStatus::Valid,
            Ok(Session::Refreshed(account)) if account.id == id => {
                if let Some(saved) = data.accounts.accounts.iter_mut().find(|a| a.id == id) {
                    *saved = account;
                }
                banner =
                    data.accounts.save(&data.dir).err().map(|e| {
                        Banner::warn(format!("Your sign-in was renewed. {}", not_saved(&e)))
                    });
                SessionStatus::Valid
            }
            // The Microsoft account now plays as a different profile; leave
            // the saved one alone.
            Ok(Session::Refreshed(_)) => SessionStatus::Unverified,
            Err(auth::Error::NeedsSignIn) => SessionStatus::Expired,
            Err(error) => {
                tracing::info!(%error, "couldn't check the Minecraft session");
                SessionStatus::Unverified
            }
        };
        data.sessions.insert(id, status);
        banner
    }
}

/// The second half of a toast when `accounts.json` couldn't be written.
fn not_saved(e: &SaveError) -> String {
    match e {
        SaveError::Blocked(_) => {
            "Not saved — accounts.json couldn't be read, so it's left untouched.".to_owned()
        }
        SaveError::Io(e) => format!("Not saved — {e}."),
    }
}
