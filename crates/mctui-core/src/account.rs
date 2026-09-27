//! `accounts.json`: signed-in Minecraft accounts and their tokens.

use std::path::Path;
use std::{fmt, fs, io};

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::paths::DataDir;
use crate::{Loaded, fsutil};

/// `accounts.json` holds a long-lived Microsoft refresh token: owner-only.
const ACCOUNTS_MODE: u32 = 0o600;

/// Taken by every mctui changing `accounts.json`, beside it.
const LOCK_FILE: &str = ".accounts.json.lock";

/// Refresh tokens this long before they expire.
const EXPIRY_BUFFER: SignedDuration = SignedDuration::from_mins(5);

/// How an account signs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountKind {
    /// A Microsoft account.
    Msa,
    /// A local name with no sign-in, for offline play.
    Offline,
    /// A kind this version doesn't know, kept verbatim so saving doesn't lose
    /// it. A missing or `null` type is `Other("")`, as Go read it.
    Other(String),
}

impl AccountKind {
    /// The id stored in `accounts.json`.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Msa => "msa",
            Self::Offline => "offline",
            Self::Other(id) => id,
        }
    }
}

/// Parses a stored id exactly as Go compared it: case-sensitive, and anything
/// unknown is kept as [`AccountKind::Other`].
impl From<&str> for AccountKind {
    fn from(id: &str) -> Self {
        match id {
            "msa" => Self::Msa,
            "offline" => Self::Offline,
            _ => Self::Other(id.to_owned()),
        }
    }
}

impl Serialize for AccountKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AccountKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let id = Option::<String>::deserialize(d)?.unwrap_or_default();
        Ok(Self::from(id.as_str()))
    }
}

/// Go read a missing `type` as `""`.
fn missing_kind() -> AccountKind {
    AccountKind::Other(String::new())
}

/// A token that never shows up in `Debug` output or logs.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// The token itself, for the one place that sends it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// A Minecraft account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// The Minecraft profile UUID.
    #[serde(default)]
    pub id: String,
    /// The Minecraft username.
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default = "missing_kind")]
    pub kind: AccountKind,
    /// The Minecraft access token.
    #[serde(default)]
    pub access_token: Secret,
    /// When `access_token` expires.
    #[serde(default, with = "crate::gotime")]
    pub expires_at: Option<Timestamp>,
    /// Refreshes the Microsoft sign-in without asking again.
    #[serde(
        default,
        deserialize_with = "non_empty_secret",
        skip_serializing_if = "Option::is_none"
    )]
    pub msa_refresh_token: Option<Secret>,
}

impl Account {
    /// Whether the access token needs refreshing: expired or within five
    /// minutes of it. Offline accounts never expire; any other account with no
    /// known expiry is treated as expired (as in the Go app).
    pub fn is_expired(&self, now: Timestamp) -> bool {
        match (&self.kind, self.expires_at) {
            (AccountKind::Offline, _) => false,
            (_, None) => true,
            (_, Some(expires)) => now.duration_until(expires) < EXPIRY_BUFFER,
        }
    }
}

/// Go omitted an empty refresh token; treat an explicit `""` the same.
fn non_empty_secret<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Secret>, D::Error> {
    let token = Option::<Secret>::deserialize(d)?;
    Ok(token.filter(|t| !t.is_empty()))
}

/// Why [`Accounts::save`] failed.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// `accounts.json` exists but couldn't be read at startup, so writing over
    /// it could destroy the tokens it holds.
    #[error("not overwriting accounts.json, which couldn't be read: {0}")]
    Blocked(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Every known account and which one is active.
#[derive(Debug, Clone, Default)]
pub struct Accounts {
    pub accounts: Vec<Account>,
    pub active_id: Option<String>,
    save_blocked: Option<String>,
}

impl Accounts {
    /// A missing file gives no accounts. A malformed one is quarantined (kept
    /// owner-only) and gives no accounts, with a notice. One that can't be read
    /// gives no accounts and a notice, and blocks saves for the session so we
    /// never overwrite tokens we couldn't read. A readable file is narrowed to
    /// owner-only (best effort; older versions wrote 0644).
    pub fn load(dir: &DataDir) -> Loaded<Accounts> {
        const CONSEQUENCE: &str = "signed out for now";
        let path = dir.accounts_file();
        let data = match crate::read_optional(&path) {
            Ok(Some(data)) => data,
            Ok(None) => return Loaded::clean(Accounts::default()),
            Err(e) => {
                return Loaded {
                    value: Accounts::blocked(e.to_string()),
                    notice: Some(crate::unreadable_notice(&path, &e, CONSEQUENCE)),
                };
            }
        };
        match serde_json::from_slice::<AccountsFile>(&data) {
            Ok(file) => {
                restrict(&path);
                Loaded::clean(Accounts::from(file))
            }
            Err(e) => {
                let (notice, moved) = crate::quarantine_malformed(&path, &e, CONSEQUENCE);
                let value = match moved {
                    Some(dest) => {
                        restrict(&dest);
                        Accounts::default()
                    }
                    // Still in place: a save would overwrite whatever it holds.
                    None => Accounts::blocked(e.to_string()),
                };
                Loaded {
                    value,
                    notice: Some(notice),
                }
            }
        }
    }

    /// Changes `accounts.json` without losing what another mctui (or a
    /// second window) saved since this one loaded it: under a lock every
    /// mctui takes to change the file, reads it again, applies `change`,
    /// writes it back as [`save`](Self::save) does, and makes `self` what was
    /// written. When the file can't be read or written, `change` is still
    /// made to `self`, for this session, and the error returned.
    pub fn update(
        &mut self,
        dir: &DataDir,
        change: impl FnOnce(&mut Accounts),
    ) -> Result<(), SaveError> {
        let current = lock(dir)
            .map_err(SaveError::from)
            .and_then(|lock| Ok((lock, read_current(dir)?)));
        let (_lock, mut current) = match current {
            Ok(locked) => locked,
            Err(e) => {
                change(self);
                return Err(e);
            }
        };
        change(&mut current);
        let saved = current.save(dir);
        *self = current;
        saved
    }

    /// Writes `accounts.json` atomically, readable only by the owner. What
    /// another mctui saved since this one loaded it is lost: prefer
    /// [`update`](Self::update).
    pub fn save(&self, dir: &DataDir) -> Result<(), SaveError> {
        if let Some(reason) = &self.save_blocked {
            return Err(SaveError::Blocked(reason.clone()));
        }
        fs::create_dir_all(dir.root())?;
        let file = AccountsFile {
            accounts: self.accounts.clone(),
            active_id: self.active_id.clone().unwrap_or_default(),
        };
        let data = serde_json::to_vec_pretty(&file).map_err(io::Error::from)?;
        fsutil::write_atomic(&dir.accounts_file(), &data, ACCOUNTS_MODE)?;
        Ok(())
    }

    /// The active account, if any.
    pub fn active(&self) -> Option<&Account> {
        let id = self.active_id.as_deref()?;
        self.accounts.iter().find(|a| a.id == id)
    }

    /// Replaces the account with the same id, or appends it. With no active
    /// account, an appended one becomes active (as in the Go app).
    pub fn upsert(&mut self, acc: Account) {
        if let Some(existing) = self.accounts.iter_mut().find(|a| a.id == acc.id) {
            *existing = acc;
            return;
        }
        if self.active_id.is_none() {
            self.active_id = Some(acc.id.clone());
        }
        self.accounts.push(acc);
    }

    /// Makes `id` the active account; `false` if there's no such account.
    pub fn set_active(&mut self, id: &str) -> bool {
        let exists = self.accounts.iter().any(|a| a.id == id);
        if exists {
            self.active_id = Some(id.to_owned());
        }
        exists
    }

    /// Removes an account, clearing the active one if it was this.
    pub fn remove(&mut self, id: &str) -> Option<Account> {
        let index = self.accounts.iter().position(|a| a.id == id)?;
        if self.active_id.as_deref() == Some(id) {
            self.active_id = None;
        }
        Some(self.accounts.remove(index))
    }

    fn blocked(reason: String) -> Self {
        Self {
            save_blocked: Some(reason),
            ..Self::default()
        }
    }
}

/// `accounts.json` as Go wrote it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountsFile {
    #[serde(default)]
    accounts: Vec<Account>,
    #[serde(default)]
    active_id: String,
}

impl From<AccountsFile> for Accounts {
    fn from(file: AccountsFile) -> Self {
        Accounts {
            accounts: file.accounts,
            active_id: Some(file.active_id).filter(|id| !id.is_empty()),
            save_blocked: None,
        }
    }
}

/// Takes the lock for changing `accounts.json`, waiting for another mctui
/// to finish with it.
fn lock(dir: &DataDir) -> io::Result<fsutil::FileLock> {
    fs::create_dir_all(dir.root())?;
    fsutil::FileLock::acquire(&dir.root().join(LOCK_FILE))
}

/// `accounts.json` as it is now, for [`Accounts::update`]: none when it's
/// missing. One that can't be read or is damaged isn't written over.
fn read_current(dir: &DataDir) -> Result<Accounts, SaveError> {
    let blocked = |e: &dyn std::fmt::Display| SaveError::Blocked(e.to_string());
    match crate::read_optional(&dir.accounts_file()) {
        Ok(Some(data)) => serde_json::from_slice::<AccountsFile>(&data)
            .map(Accounts::from)
            .map_err(|e| blocked(&e)),
        Ok(None) => Ok(Accounts::default()),
        Err(e) => Err(blocked(&e)),
    }
}

/// Narrows a file holding tokens to owner-only (0644 becomes 0600; 0400 stays).
/// Best effort: a failure here must not lose accounts we just read.
fn restrict(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let Ok(meta) = fs::metadata(path) else {
            return;
        };
        let mode = meta.permissions().mode() & 0o777;
        if mode & !ACCOUNTS_MODE == 0 {
            return;
        }
        let narrowed = fs::Permissions::from_mode(mode & ACCOUNTS_MODE);
        if let Err(e) = fs::set_permissions(path, narrowed) {
            tracing::warn!(path = %path.display(), error = %e, "couldn't restrict permissions");
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msa(id: &str) -> Account {
        Account {
            id: id.to_owned(),
            name: format!("{id}-name"),
            kind: AccountKind::Msa,
            access_token: Secret::new("test-access-token"),
            expires_at: None,
            msa_refresh_token: Some(Secret::new("test-refresh-token")),
        }
    }

    #[test]
    fn debug_never_shows_tokens() {
        let debug = format!("{:?}", msa("a"));
        assert!(!debug.contains("test-access-token"), "{debug}");
        assert!(!debug.contains("test-refresh-token"), "{debug}");
        assert!(debug.contains("Secret(<redacted>)"));
    }

    #[test]
    fn expiry_has_a_five_minute_buffer() {
        let now: Timestamp = "2026-09-27T12:00:00Z".parse().unwrap();
        let mut acc = msa("a");
        assert!(acc.is_expired(now), "no expiry is expired");
        acc.expires_at = Some(now + SignedDuration::from_mins(4));
        assert!(acc.is_expired(now));
        acc.expires_at = Some(now + SignedDuration::from_mins(6));
        assert!(!acc.is_expired(now));
        acc.kind = AccountKind::Offline;
        acc.expires_at = None;
        assert!(!acc.is_expired(now));
    }

    #[test]
    fn upsert_set_active_remove() {
        let mut accounts = Accounts::default();
        accounts.upsert(msa("a"));
        assert_eq!(
            accounts.active_id.as_deref(),
            Some("a"),
            "first account becomes active"
        );
        accounts.upsert(msa("b"));
        assert_eq!(accounts.active_id.as_deref(), Some("a"));

        let mut renamed = msa("b");
        renamed.name = "renamed".to_owned();
        accounts.upsert(renamed);
        assert_eq!(accounts.accounts.len(), 2);
        assert_eq!(accounts.accounts[1].name, "renamed");

        assert!(accounts.set_active("b"));
        assert!(!accounts.set_active("nope"));
        assert_eq!(accounts.active().map(|a| a.id.as_str()), Some("b"));

        assert_eq!(accounts.remove("b").map(|a| a.id), Some("b".to_owned()));
        assert_eq!(accounts.active_id, None);
        assert!(accounts.remove("b").is_none());
    }

    fn ids(accounts: &Accounts) -> Vec<&str> {
        accounts.accounts.iter().map(|a| a.id.as_str()).collect()
    }

    #[test]
    fn updates_keep_what_another_window_saved() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        // Two windows load the same (missing) file.
        let mut first = Accounts::load(&data).value;
        let mut second = Accounts::load(&data).value;
        first.update(&data, |a| a.upsert(msa("a"))).unwrap();
        second.update(&data, |a| a.upsert(msa("b"))).unwrap();
        assert_eq!(ids(&second), ["a", "b"], "the second sees the first's too");
        assert_eq!(second.active_id.as_deref(), Some("a"));
        assert_eq!(ids(&Accounts::load(&data).value), ["a", "b"]);
    }

    #[test]
    fn concurrent_updates_are_all_kept() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        std::thread::scope(|s| {
            for i in 0..8 {
                let data = &data;
                s.spawn(move || {
                    let mut accounts = Accounts::default();
                    accounts
                        .update(data, |a| a.upsert(msa(&i.to_string())))
                        .unwrap();
                });
            }
        });
        let mut saved = Accounts::load(&data).value;
        saved.accounts.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(ids(&saved), ["0", "1", "2", "3", "4", "5", "6", "7"]);
    }

    #[test]
    fn an_update_that_cannot_read_the_file_changes_only_this_session() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        fs::write(data.accounts_file(), "{\"accounts\": [").unwrap();
        let mut accounts = Accounts::default();
        let err = accounts.update(&data, |a| a.upsert(msa("a"))).unwrap_err();
        assert!(matches!(err, SaveError::Blocked(_)), "{err}");
        assert_eq!(ids(&accounts), ["a"]);
        assert_eq!(
            fs::read_to_string(data.accounts_file()).unwrap(),
            "{\"accounts\": [",
            "left as it was"
        );
    }

    #[test]
    fn empty_refresh_token_reads_as_none() {
        let json = r#"{"id":"a","name":"n","type":"msa","accessToken":"t","expiresAt":"0001-01-01T00:00:00Z","msaRefreshToken":""}"#;
        let acc: Account = serde_json::from_str(json).unwrap();
        assert_eq!(acc.msa_refresh_token, None);
        assert_eq!(acc.expires_at, None);
        let back = serde_json::to_string(&acc).unwrap();
        assert!(!back.contains("msaRefreshToken"), "{back}");
    }

    #[test]
    fn kind_ids() {
        assert_eq!(AccountKind::from("msa"), AccountKind::Msa);
        assert_eq!(AccountKind::from("offline"), AccountKind::Offline);
        assert_eq!(
            AccountKind::from("MSA"),
            AccountKind::Other("MSA".to_owned())
        );
        assert_eq!(AccountKind::Other("mojang".to_owned()).as_str(), "mojang");
        let kind: AccountKind = serde_json::from_str("null").unwrap();
        assert_eq!(kind, AccountKind::Other(String::new()));
        assert!(serde_json::from_str::<AccountKind>("7").is_err());
    }

    #[test]
    fn unknown_and_missing_kinds_load_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        fs::write(
            data.accounts_file(),
            r#"{"accounts":[
                {"id":"a","name":"A","type":"msa"},
                {"id":"b","name":"B","type":"mojang","expiresAt":"2999-01-01T00:00:00Z"},
                {"id":"c","name":"C"},
                {"id":"d","name":"D","type":null}
            ],"activeId":"b"}"#,
        )
        .unwrap();

        let loaded = Accounts::load(&data);
        assert!(loaded.notice.is_none(), "{:?}", loaded.notice);
        let kinds: Vec<_> = loaded.value.accounts.iter().map(|a| &a.kind).collect();
        let other = |s: &str| AccountKind::Other(s.to_owned());
        assert_eq!(
            kinds,
            [&AccountKind::Msa, &other("mojang"), &other(""), &other("")]
        );
        let now = Timestamp::now();
        assert!(!loaded.value.accounts[1].is_expired(now), "expiry decides");
        assert!(loaded.value.accounts[2].is_expired(now), "no expiry");

        loaded.value.save(&data).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(data.accounts_file()).unwrap()).unwrap();
        let saved_kinds: Vec<_> = saved["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["type"].as_str().unwrap())
            .collect();
        assert_eq!(saved_kinds, ["msa", "mojang", "", ""]);
        assert_eq!(Accounts::load(&data).value.accounts, loaded.value.accounts);
    }

    #[test]
    fn malformed_file_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        fs::write(data.accounts_file(), "{\"accounts\": [").unwrap();
        let loaded = Accounts::load(&data);
        assert!(loaded.value.accounts.is_empty());
        let notice = loaded.notice.unwrap();
        assert!(
            notice.starts_with("accounts.json was damaged — signed out for now."),
            "{notice}"
        );
        assert!(!data.accounts_file().exists());
        // Saves aren't blocked: the old file is safely aside.
        loaded.value.save(&data).unwrap();
    }

    #[cfg(unix)]
    mod unix {
        use std::os::unix::fs::PermissionsExt;

        use super::*;

        #[test]
        fn saved_file_is_owner_only() {
            let dir = tempfile::tempdir().unwrap();
            let data = DataDir::new(dir.path());
            let mut accounts = Accounts::default();
            accounts.upsert(msa("a"));
            accounts.save(&data).unwrap();
            let mode = fs::metadata(data.accounts_file())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        #[test]
        fn updated_file_is_owner_only() {
            let dir = tempfile::tempdir().unwrap();
            let data = DataDir::new(dir.path());
            fs::write(data.accounts_file(), r#"{"accounts":[],"activeId":""}"#).unwrap();
            fs::set_permissions(data.accounts_file(), fs::Permissions::from_mode(0o644)).unwrap();
            let mut accounts = Accounts::default();
            accounts.update(&data, |a| a.upsert(msa("a"))).unwrap();
            let mode = fs::metadata(data.accounts_file())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        #[test]
        fn load_tightens_an_open_file() {
            let dir = tempfile::tempdir().unwrap();
            let data = DataDir::new(dir.path());
            fs::write(data.accounts_file(), r#"{"accounts":[],"activeId":""}"#).unwrap();
            fs::set_permissions(data.accounts_file(), fs::Permissions::from_mode(0o644)).unwrap();
            let loaded = Accounts::load(&data);
            assert!(loaded.notice.is_none());
            let mode = fs::metadata(data.accounts_file())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        #[test]
        fn unreadable_file_blocks_saves() {
            let dir = tempfile::tempdir().unwrap();
            let data = DataDir::new(dir.path());
            let path = data.accounts_file();
            fs::write(&path, r#"{"accounts":[],"activeId":""}"#).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            if fs::read(&path).is_ok() {
                return; // running as root: permissions don't apply
            }

            let loaded = Accounts::load(&data);
            let notice = loaded.notice.unwrap();
            assert!(
                notice.starts_with("Couldn't read accounts.json ("),
                "{notice}"
            );
            assert!(notice.ends_with("— signed out for now."), "{notice}");
            let err = loaded.value.save(&data).unwrap_err();
            assert!(matches!(err, SaveError::Blocked(_)), "{err}");
            // Left exactly where it was.
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                r#"{"accounts":[],"activeId":""}"#
            );
        }
    }
}
