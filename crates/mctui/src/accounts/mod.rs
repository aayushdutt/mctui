//! Accounts: who you play as, signing in with Microsoft, and whether each
//! Microsoft sign-in still works.
//!
//! Screens ask for changes with a [`Change`]; the app applies it, saves
//! `accounts.json` and keeps sessions checked ([`Checks`]). So a sign-in that
//! finishes is saved by the app, not by a screen that may be gone.

mod list;
mod offline;
mod session;
mod signin;
#[cfg(test)]
pub(crate) mod tests;

use mctui_core::account::{Account, AccountKind};
use mctui_core::auth::{self, DeviceCode, Session};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

pub use list::AccountList;
pub use offline::OfflineProfile;
pub use session::{Checks, SessionStatus};
pub use signin::SignIn;

use crate::data::Data;
use crate::theme::Theme;
use crate::ui;

/// Background work reporting back: a sign-in's steps to its screen, session
/// checks to the app.
#[derive(Debug)]
pub enum Msg {
    /// A step of sign-in attempt `attempt`.
    SignIn { attempt: u64, step: Step },
    /// Session check `seq` of account `id` finished.
    Session {
        id: String,
        seq: u64,
        result: auth::Result<Session>,
    },
}

/// How a sign-in is going.
#[derive(Debug)]
pub enum Step {
    /// Microsoft handed out a code for the player to enter.
    Code(DeviceCode),
    /// The player entered it; now trading Microsoft's tokens for Minecraft's.
    SigningIn,
    Done(auth::Result<Account>),
}

/// A change to the accounts, applied (and saved) by the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Play as this account from now on.
    Activate(String),
    Remove(String),
    /// A new offline profile with this name.
    AddOffline(String),
    /// A Microsoft sign-in finished: add or update it, and make it active.
    SignedIn(Box<Account>),
}

/// Whether playing online has to sign in first: there's no account, or its
/// Microsoft sign-in is known to have expired. A session that merely
/// couldn't be checked doesn't count; the launch finds out for sure.
pub fn needs_sign_in(data: &Data) -> bool {
    data.accounts
        .active()
        .is_none_or(|account| match account.kind {
            AccountKind::Offline => false,
            AccountKind::Msa => data.sessions.get(&account.id) == Some(&SessionStatus::Expired),
            AccountKind::Other(_) => true,
        })
}

/// The header's right side: who you're playing as, and how their sign-in is.
pub fn status_line(data: &Data, t: &Theme) -> Line<'static> {
    let Some(account) = data.accounts.active() else {
        return Line::styled("Not signed in", Style::new().fg(t.dim));
    };
    let name = Span::styled(account.name.clone(), Style::new().fg(t.text));
    let glyph = |glyph: &str, fg| Span::styled(format!("{glyph} "), Style::new().fg(fg));
    let note = |note: &str, fg| Span::styled(format!(" · {note}"), Style::new().fg(fg));
    let spans = match (&account.kind, data.sessions.get(&account.id)) {
        (AccountKind::Offline, _) => vec![name, note("offline", t.dim)],
        (AccountKind::Msa, Some(SessionStatus::Checking)) => {
            vec![glyph("○", t.muted), name, note("checking…", t.dim)]
        }
        (AccountKind::Msa, Some(SessionStatus::Expired)) => vec![
            glyph(ui::WARN, t.warning),
            name,
            note("sign-in expired", t.warning),
        ],
        (AccountKind::Msa, Some(SessionStatus::Unverified)) => {
            vec![
                glyph(ui::WARN, t.warning),
                name,
                note("can't verify", t.dim),
            ]
        }
        (AccountKind::Msa, Some(SessionStatus::Valid) | None) => {
            vec![glyph(ui::DOT, t.success), name]
        }
        (AccountKind::Other(_), _) => vec![name],
    };
    Line::from(spans)
}
