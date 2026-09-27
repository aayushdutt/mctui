//! Who the game runs as.

use std::fmt::Write as _;

use md5::{Digest as _, Md5};

use crate::account::{Account, AccountKind, Secret};

/// The name an offline player gets when none is given.
const DEFAULT_NAME: &str = "Player";

/// What the game passes for a missing access token.
const NO_TOKEN: &str = "0";

/// The player the game runs as: a signed-in Microsoft account, or an offline
/// name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    name: String,
    uuid: String,
    access_token: Option<Secret>,
    xuid: Option<String>,
}

impl Identity {
    /// A signed-in player: the Minecraft profile's name and UUID, a
    /// Minecraft access token, and the Xbox user id if known.
    pub fn online(
        name: impl Into<String>,
        uuid: impl Into<String>,
        access_token: Secret,
        xuid: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            uuid: uuid.into(),
            access_token: Some(access_token),
            xuid,
        }
    }

    /// An offline player named `name` ("Player" when blank), with the UUID
    /// offline servers give that name (see [`offline_uuid`]).
    pub fn offline(name: &str) -> Self {
        let name = match name.trim() {
            "" => DEFAULT_NAME,
            name => name,
        };
        Self {
            uuid: offline_uuid(name),
            name: name.to_owned(),
            access_token: None,
            xuid: None,
        }
    }

    /// Plays as `account`: online for a Microsoft account, else offline under
    /// its name.
    pub fn for_account(account: &Account) -> Self {
        match account.kind {
            AccountKind::Msa => Self::online(
                &account.name,
                &account.id,
                account.access_token.clone(),
                None,
            ),
            _ => Self::offline(&account.name),
        }
    }

    /// Whether the player is signed in.
    pub fn is_online(&self) -> bool {
        self.access_token.is_some()
    }

    /// The player's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The player's UUID, as 32 hex digits for an offline player.
    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    /// The Minecraft access token; `0` offline.
    pub(super) fn access_token(&self) -> &str {
        self.access_token.as_ref().map_or(NO_TOKEN, Secret::expose)
    }

    /// The token to hide wherever the command line is shown.
    pub(super) fn secret(&self) -> Option<&Secret> {
        self.access_token.as_ref()
    }

    /// The Xbox user id; empty, which the game reads as absent, when unknown.
    pub(super) fn xuid(&self) -> &str {
        self.xuid.as_deref().unwrap_or_default()
    }

    /// `msa` for a Microsoft account. Offline it's `legacy`, the value the
    /// game assumes when it isn't told (from 26.1 it isn't asked at all).
    pub(super) fn user_type(&self) -> &'static str {
        if self.is_online() { "msa" } else { "legacy" }
    }
}

/// The UUID offline servers give the player `name`: Java's
/// `UUID.nameUUIDFromBytes("OfflinePlayer:<name>")` (an MD5 name-based UUID,
/// version 3), as 32 hex digits like Mojang's profile ids.
pub fn offline_uuid(name: &str) -> String {
    let mut bytes: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}")).into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // version 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // IETF variant
    bytes.iter().fold(String::with_capacity(32), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuids_match_javas() {
        // From Java's UUID.nameUUIDFromBytes, as offline servers compute them.
        for (name, want) in [
            ("Notch", "b50ad385829d3141a2167e7d7539ba7f"),
            ("Player", "a01e3843e5213998958af459800e4d11"),
            ("Steve", "5627dd98e6be3c21b8a8e92344183641"),
        ] {
            assert_eq!(offline_uuid(name), want, "{name}");
        }
    }

    #[test]
    fn offline_identity() {
        let steve = Identity::offline(" Steve ");
        assert_eq!(steve.name(), "Steve");
        assert_eq!(steve.uuid(), offline_uuid("Steve"));
        assert_eq!(steve.access_token(), "0");
        assert_eq!((steve.xuid(), steve.user_type()), ("", "legacy"));
        assert!(!steve.is_online() && steve.secret().is_none());

        let blank = Identity::offline("  ");
        assert_eq!(blank.name(), "Player");
        assert_eq!(blank.uuid(), offline_uuid("Player"));
    }

    #[test]
    fn accounts_play_online_or_offline() {
        let mut account = Account {
            id: "069a79f444e94726a5befca90e38aaf5".into(),
            name: "Notch".into(),
            kind: AccountKind::Msa,
            access_token: Secret::new("tok"),
            expires_at: None,
            msa_refresh_token: None,
        };
        let online = Identity::for_account(&account);
        assert!(online.is_online());
        assert_eq!(online.uuid(), "069a79f444e94726a5befca90e38aaf5");
        assert_eq!((online.access_token(), online.user_type()), ("tok", "msa"));

        account.kind = AccountKind::Offline;
        let offline = Identity::for_account(&account);
        assert_eq!(offline, Identity::offline("Notch"));
    }
}
