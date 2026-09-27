//! Microsoft sign-in for Minecraft: the device-code flow, silent refresh, and
//! the session check.
//!
//! Signing in takes three calls: [`Auth::start`] gets a code for the player to
//! enter on Microsoft's site, [`Auth::poll`] waits until they have, and
//! [`Auth::finish`] trades Microsoft's tokens for a Minecraft [`Account`] by way
//! of Xbox Live. Later, [`Auth::refresh`] renews a saved account without asking
//! the player, and [`Auth::ensure_session`] decides whether an account needs
//! that before playing online.
//!
//! Every call is cancelled by dropping its future. Nothing is retried here:
//! token requests aren't idempotent, and the caller knows best whether to try
//! again. Tokens stay in [`Secret`]s and never appear in errors or logs.

use std::time::Duration;

use bytes::Bytes;
use jiff::{SignedDuration, Timestamp};
use reqwest::StatusCode;
use reqwest::header::ACCEPT;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::account::{Account, AccountKind, Secret};
use crate::net::Http;

/// Xbox Live sign-in, plus a refresh token so the player needn't sign in again.
const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// Seconds between polls when Microsoft doesn't say (RFC 8628 §3.2).
const DEFAULT_INTERVAL: u32 = 5;
/// Seconds added to the poll interval on each `slow_down` (RFC 8628 §3.5).
const SLOW_DOWN: u32 = 5;
/// Refresh refusals that mean the saved sign-in is gone for good. Anything
/// else (a bad client id, rate limits, outages) isn't fixed by signing in again.
const SIGN_IN_AGAIN: [&str; 3] = ["invalid_grant", "interaction_required", "consent_required"];

const MICROSOFT: &str = "Microsoft";
const XBOX: &str = "Xbox Live";
const MINECRAFT: &str = "Minecraft";

/// Where each sign-in step is sent. The default is the real services.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// Microsoft's device-code endpoint, which hands out sign-in codes.
    pub device_code: String,
    /// Microsoft's token endpoint, for polling and refreshing.
    pub token: String,
    /// Xbox Live user authentication.
    pub xbox_user: String,
    /// Xbox Live security tokens (XSTS) for Minecraft.
    pub xsts: String,
    /// Minecraft's sign-in with an Xbox Live identity.
    pub minecraft_login: String,
    /// The signed-in player's Minecraft profile.
    pub minecraft_profile: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        let microsoft = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
        Self {
            device_code: format!("{microsoft}/devicecode"),
            token: format!("{microsoft}/token"),
            xbox_user: "https://user.auth.xboxlive.com/user/authenticate".to_owned(),
            xsts: "https://xsts.auth.xboxlive.com/xsts/authorize".to_owned(),
            minecraft_login: "https://api.minecraftservices.com/authentication/login_with_xbox"
                .to_owned(),
            minecraft_profile: "https://api.minecraftservices.com/minecraft/profile".to_owned(),
        }
    }
}

/// Signs players in with Microsoft. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Auth {
    http: Http,
    endpoints: Endpoints,
    /// How long one second of Microsoft's poll timing lasts.
    poll_unit: Duration,
}

impl Auth {
    /// Signs in against the real services.
    pub fn new(http: Http) -> Self {
        Self::with_endpoints(http, Endpoints::default())
    }

    /// Signs in against `endpoints` instead, such as a mock server.
    pub fn with_endpoints(http: Http, endpoints: Endpoints) -> Self {
        Self {
            http,
            endpoints,
            poll_unit: Duration::from_secs(1),
        }
    }

    /// Makes each second of the wait between polls, which Microsoft sets in
    /// seconds, last `unit` instead. Tests use a millisecond. A code's expiry
    /// is unaffected.
    #[must_use]
    pub fn with_poll_unit(self, unit: Duration) -> Self {
        Self {
            poll_unit: unit,
            ..self
        }
    }

    /// Starts a sign-in: gets a code for the player to enter on Microsoft's
    /// site. `client_id` is the Microsoft app registration to sign in with.
    pub async fn start(&self, client_id: &str) -> Result<DeviceCode> {
        let form = [("client_id", client_id), ("scope", SCOPE)];
        let request = self
            .http
            .client()
            .post(&self.endpoints.device_code)
            .form(&form);
        let reply: DeviceCodeReply = send(MICROSOFT, request).await?.json()?;
        Ok(DeviceCode {
            user_code: reply.user_code,
            verification_uri: reply.verification_uri,
            expires_at: from_now(reply.expires_in),
            interval: self.poll_unit.saturating_mul(reply.interval.max(1)),
            secret: reply.device_code,
            client_id: client_id.to_owned(),
        })
    }

    /// Waits for the player to enter `code`, checking at the interval
    /// Microsoft asked for, and longer when it says to slow down. Dropped
    /// connections, outages and garbled answers don't end the wait; only the
    /// player finishing or declining, or the code expiring, does. After an
    /// error it can be called again with the same code until that expires.
    pub async fn poll(&self, code: &DeviceCode) -> Result<MsTokens> {
        let form = [
            ("grant_type", DEVICE_CODE_GRANT),
            ("client_id", code.client_id.as_str()),
            ("device_code", code.secret.expose()),
        ];
        let mut interval = code.interval;
        loop {
            if Timestamp::now() >= code.expires_at {
                return Err(Error::Expired);
            }
            tokio::time::sleep(interval).await;
            match self.grant(&form).await {
                Ok(Grant::Tokens(tokens)) => return Ok(tokens),
                Ok(Grant::Refused { error_code, error }) => match error_code.as_str() {
                    "authorization_pending" => {}
                    "slow_down" => {
                        interval =
                            interval.saturating_add(self.poll_unit.saturating_mul(SLOW_DOWN));
                    }
                    "expired_token" => return Err(Error::Expired),
                    "authorization_declined" | "access_denied" => return Err(Error::Denied),
                    _ => return Err(error),
                },
                Err(error) => tracing::debug!(?error, "sign-in poll failed; trying again"),
            }
        }
    }

    /// Finishes a sign-in: trades Microsoft's tokens for a Minecraft token
    /// through Xbox Live, then fetches the player's profile.
    pub async fn finish(&self, tokens: MsTokens) -> Result<Account> {
        let user_request = XboxRequest::user(&tokens.access_token);
        let user = self.xbox(&self.endpoints.xbox_user, &user_request).await?;
        let xsts_request = XboxRequest::xsts(&user.token);
        let xsts = self.xbox(&self.endpoints.xsts, &xsts_request).await?;

        let login = MinecraftLogin::new(&xsts);
        let request = self
            .http
            .client()
            .post(&self.endpoints.minecraft_login)
            .json(&login);
        let minecraft: MinecraftToken = send(MINECRAFT, request).await?.json()?;
        let expires_at = from_now(minecraft.expires_in);

        let profile = self.fetch_profile(&minecraft.access_token).await?;
        Ok(Account {
            id: profile.id,
            name: profile.name,
            kind: AccountKind::Msa,
            access_token: minecraft.access_token,
            expires_at: Some(expires_at),
            msa_refresh_token: tokens.refresh_token,
        })
    }

    /// Renews `account` with its saved Microsoft refresh token, without asking
    /// the player, and fetches its profile again. [`Error::NeedsSignIn`] means
    /// the saved sign-in is gone (or there never was one); after any other
    /// error it may still work later.
    pub async fn refresh(&self, account: &Account, client_id: &str) -> Result<Account> {
        let Some(refresh_token) = &account.msa_refresh_token else {
            return Err(Error::NeedsSignIn);
        };
        let form = [
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token.expose()),
            ("scope", SCOPE),
        ];
        let mut tokens = match self.grant(&form).await? {
            Grant::Tokens(tokens) => tokens,
            Grant::Refused { error_code, .. } if SIGN_IN_AGAIN.contains(&error_code.as_str()) => {
                return Err(Error::NeedsSignIn);
            }
            Grant::Refused { error, .. } => return Err(error),
        };
        // Microsoft usually sends a new refresh token, but needn't.
        tokens
            .refresh_token
            .get_or_insert_with(|| refresh_token.clone());
        self.finish(tokens).await
    }

    /// The account's Minecraft profile, which also checks its token.
    /// [`Error::SessionExpired`] means Minecraft rejected the token, whatever
    /// its expiry says: [`refresh`](Self::refresh) it, and sign in again if
    /// that fails with [`Error::NeedsSignIn`].
    pub async fn profile(&self, account: &Account) -> Result<Profile> {
        self.fetch_profile(&account.access_token).await
    }

    /// Makes sure `account` can play online for at least `valid_for` more:
    /// checks its token with Minecraft, and refreshes it if it expires sooner
    /// or Minecraft rejects it. A status check needs a few minutes; a launch
    /// needs a few hours, since joining servers uses the token all session.
    pub async fn ensure_session(
        &self,
        account: &Account,
        client_id: &str,
        valid_for: SignedDuration,
    ) -> Result<Session> {
        let lasts = account
            .expires_at
            .is_some_and(|at| Timestamp::now().duration_until(at) >= valid_for);
        if lasts {
            match self.profile(account).await {
                Ok(_) => return Ok(Session::Valid),
                Err(Error::SessionExpired) => {}
                Err(e) => return Err(e),
            }
        }
        self.refresh(account, client_id)
            .await
            .map(Session::Refreshed)
    }

    /// Asks the token endpoint for tokens, once.
    async fn grant(&self, form: &[(&str, &str)]) -> Result<Grant> {
        let request = self.http.client().post(&self.endpoints.token).form(form);
        let reply = send(MICROSOFT, request).await?;
        if reply.status.is_client_error()
            && let Some(error_code) = reply.error_body().error
        {
            let error = reply.failure();
            return Ok(Grant::Refused { error_code, error });
        }
        let tokens: TokenReply = reply.json()?;
        if tokens.access_token.is_empty() {
            return Err(reply.error("no access token"));
        }
        Ok(Grant::Tokens(MsTokens {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token.filter(|t| !t.is_empty()),
        }))
    }

    /// One Xbox Live step: each trades a token for a token and user hash.
    async fn xbox(&self, url: &str, body: &impl Serialize) -> Result<XboxToken> {
        let request = self
            .http
            .client()
            .post(url)
            .header(ACCEPT, "application/json")
            .json(body);
        let reply = send(XBOX, request).await?;
        if let Some(code) = reply.error_body().xerr {
            return Err(Error::Xbox(XboxRefusal::from_code(code)));
        }
        let XboxReply {
            token,
            display_claims,
        } = reply.json()?;
        let user_hash = display_claims
            .xui
            .into_iter()
            .next()
            .map(|claims| claims.uhs)
            .ok_or_else(|| reply.error("no user hash"))?;
        Ok(XboxToken { token, user_hash })
    }

    async fn fetch_profile(&self, access_token: &Secret) -> Result<Profile> {
        let request = self
            .http
            .client()
            .get(&self.endpoints.minecraft_profile)
            .bearer_auth(access_token.expose());
        let reply = send(MINECRAFT, request).await?;
        match reply.status {
            StatusCode::UNAUTHORIZED => Err(Error::SessionExpired),
            StatusCode::NOT_FOUND => Err(Error::NoMinecraft),
            _ => reply.json(),
        }
    }
}

/// A sign-in in progress: the code the player enters on Microsoft's site.
#[derive(Debug, Clone)]
pub struct DeviceCode {
    /// The code to show the player.
    pub user_code: String,
    /// Where the player enters it, such as <https://www.microsoft.com/link>.
    pub verification_uri: String,
    /// When the code stops working; [`Auth::poll`] gives up then.
    pub expires_at: Timestamp,
    /// How long [`Auth::poll`] waits between checks, as Microsoft asked.
    pub interval: Duration,
    /// The code [`Auth::poll`] sends. Whoever holds it gets the tokens.
    secret: Secret,
    client_id: String,
}

/// Microsoft's tokens for a player who has signed in, for [`Auth::finish`].
#[derive(Debug, Clone)]
pub struct MsTokens {
    access_token: Secret,
    refresh_token: Option<Secret>,
}

/// A Minecraft: Java Edition profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Profile {
    /// The player's UUID, without dashes.
    pub id: String,
    /// The player's name.
    pub name: String,
}

/// The outcome of [`Auth::ensure_session`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a refreshed account has to be saved"]
pub enum Session {
    /// The saved token works and lasts long enough.
    Valid,
    /// The token was refreshed: save this account in place of the old one.
    Refreshed(Account),
}

/// Why signing in, refreshing, or checking a session failed. The messages are
/// written for the player and never include tokens.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The code expired before the player entered it; start again.
    #[error("the sign-in code expired — start again for a new one")]
    Expired,
    /// The player declined to sign in.
    #[error("sign-in was declined")]
    Denied,
    /// The saved Microsoft sign-in was revoked or ran out, or there is none:
    /// the player has to sign in again.
    #[error("your Microsoft sign-in has expired — sign in again")]
    NeedsSignIn,
    /// Minecraft rejected the access token. Refresh it, and sign in again if
    /// that fails with [`Error::NeedsSignIn`].
    #[error("your Minecraft session has expired")]
    SessionExpired,
    /// Xbox Live won't let this account play.
    #[error(transparent)]
    Xbox(XboxRefusal),
    /// The account doesn't own Minecraft: Java Edition, or (with Game Pass)
    /// hasn't set up a profile yet.
    #[error(
        "this account doesn't own Minecraft: Java Edition — with Game Pass, \
         open the official launcher once to set up a profile"
    )]
    NoMinecraft,
    /// A service couldn't be reached; worth trying again.
    #[error("couldn't reach {service} — check your connection")]
    Network {
        /// Which service: Microsoft, Xbox Live, or Minecraft.
        service: &'static str,
        /// What went wrong on the way.
        #[source]
        source: reqwest::Error,
    },
    /// A service answered with an error or something unexpected.
    #[error("{service} answered {status}: {reason}")]
    Service {
        /// Which service: Microsoft, Xbox Live, or Minecraft.
        service: &'static str,
        /// The HTTP status it answered with.
        status: StatusCode,
        /// A short reason from its answer.
        reason: String,
    },
}

/// Why Xbox Live refused an account, from its `XErr` code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum XboxRefusal {
    /// The Microsoft account has no Xbox profile yet.
    #[error(
        "this Microsoft account has no Xbox profile yet — sign in at xbox.com \
         once to create one, then try again"
    )]
    NoAccount,
    /// Xbox Live isn't available in the account's country.
    #[error("Xbox Live isn't available in this account's country or region")]
    Region,
    /// The account must be verified as an adult on xbox.com (South Korea).
    #[error("this account needs adult verification on xbox.com before it can play")]
    AdultVerification,
    /// The account's age must be verified on xbox.com (South Korea).
    #[error("this account needs age verification on xbox.com before it can play")]
    AgeVerification,
    /// A child account that an adult must add to a Microsoft family.
    #[error(
        "this is a child account — an adult needs to add it to a Microsoft \
         family before it can play"
    )]
    Child,
    /// Any other refusal, by its code.
    #[error("Xbox Live refused this account (error {0})")]
    Other(u64),
}

impl XboxRefusal {
    fn from_code(code: u64) -> Self {
        match code {
            2_148_916_233 => Self::NoAccount,
            2_148_916_235 => Self::Region,
            2_148_916_236 => Self::AdultVerification,
            2_148_916_237 => Self::AgeVerification,
            2_148_916_238 => Self::Child,
            other => Self::Other(other),
        }
    }
}

/// The result of an [`Auth`] call.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The token endpoint's answer to one request.
enum Grant {
    Tokens(MsTokens),
    /// An OAuth error such as `authorization_pending`, and the
    /// [`Error::Service`] it amounts to if the caller doesn't expect it.
    Refused {
        error_code: String,
        error: Error,
    },
}

/// Sends `request` once and reads the whole answer. Getting none is an
/// [`Error::Network`]; the status is the caller's to judge.
async fn send(service: &'static str, request: reqwest::RequestBuilder) -> Result<Reply> {
    let network = |source| Error::Network { service, source };
    let response = request.send().await.map_err(network)?;
    let status = response.status();
    let body = response.bytes().await.map_err(network)?;
    Ok(Reply {
        service,
        status,
        body,
    })
}

/// A service's whole answer to one request.
struct Reply {
    service: &'static str,
    status: StatusCode,
    body: Bytes,
}

impl Reply {
    /// Decodes a successful answer; anything else is an [`Error::Service`].
    fn json<T: DeserializeOwned>(&self) -> Result<T> {
        if !self.status.is_success() {
            return Err(self.failure());
        }
        serde_json::from_slice(&self.body)
            .map_err(|e| self.error(format!("unexpected response ({e})")))
    }

    /// The error details in a failed answer; empty for success or no details.
    fn error_body(&self) -> ErrorBody {
        if self.status.is_success() {
            return ErrorBody::default();
        }
        serde_json::from_slice(&self.body).unwrap_or_default()
    }

    /// An [`Error::Service`] for a failed answer, with the best reason it gives.
    fn failure(&self) -> Error {
        let busy = self.status.is_server_error() || self.status == StatusCode::TOO_MANY_REQUESTS;
        match self.error_body().reason() {
            Some(reason) => self.error(reason),
            None if busy => self.error("try again later"),
            None => self.error("unexpected response"),
        }
    }

    fn error(&self, reason: impl Into<String>) -> Error {
        Error::Service {
            service: self.service,
            status: self.status,
            reason: reason.into(),
        }
    }
}

/// The error fields the sign-in services send; each uses some of them.
#[derive(Default, Deserialize)]
struct ErrorBody {
    /// OAuth's error code, such as `invalid_grant`; Minecraft's error type.
    error: Option<String>,
    /// OAuth's explanation, a line of text and then trace ids.
    error_description: Option<String>,
    /// Minecraft's explanation.
    #[serde(rename = "errorMessage")]
    error_message: Option<String>,
    /// Xbox Live's refusal code.
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
}

impl ErrorBody {
    /// Minecraft's message, or the OAuth code with its explanation.
    fn reason(&self) -> Option<String> {
        let line = |text: &Option<String>| {
            text.as_deref()
                .map(first_line)
                .filter(|line| !line.is_empty())
        };
        if let Some(message) = line(&self.error_message) {
            return Some(message);
        }
        let code = self.error.as_deref().filter(|code| !code.is_empty())?;
        Some(match line(&self.error_description) {
            Some(description) => format!("{code}: {description}"),
            None => code.to_owned(),
        })
    }
}

/// The first line of `text`, trimmed and cut to a readable length.
fn first_line(text: &str) -> String {
    const MAX_CHARS: usize = 160;
    let line = text.lines().next().unwrap_or_default().trim();
    match line.char_indices().nth(MAX_CHARS) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_owned(),
    }
}

/// The time `secs` seconds from now, for a lifetime a service reported.
fn from_now(secs: u64) -> Timestamp {
    let secs = i64::try_from(secs).unwrap_or(i64::MAX);
    Timestamp::now()
        .saturating_add(SignedDuration::from_secs(secs))
        .unwrap_or(Timestamp::MAX)
}

#[derive(Deserialize)]
struct DeviceCodeReply {
    device_code: Secret,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u32,
}

fn default_interval() -> u32 {
    DEFAULT_INTERVAL
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: Secret,
    refresh_token: Option<Secret>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct XboxRequest<P> {
    properties: P,
    relying_party: &'static str,
    token_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UserProperties {
    auth_method: &'static str,
    site_name: &'static str,
    rps_ticket: Secret,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct XstsProperties<'a> {
    sandbox_id: &'static str,
    user_tokens: [&'a str; 1],
}

impl XboxRequest<UserProperties> {
    /// Trades a Microsoft access token for an Xbox Live user token.
    fn user(ms_access_token: &Secret) -> Self {
        Self {
            properties: UserProperties {
                auth_method: "RPS",
                site_name: "user.auth.xboxlive.com",
                rps_ticket: Secret::new(format!("d={}", ms_access_token.expose())),
            },
            relying_party: "http://auth.xboxlive.com",
            token_type: "JWT",
        }
    }
}

impl<'a> XboxRequest<XstsProperties<'a>> {
    /// Trades an Xbox Live user token for an XSTS token for Minecraft.
    fn xsts(user_token: &'a Secret) -> Self {
        Self {
            properties: XstsProperties {
                sandbox_id: "RETAIL",
                user_tokens: [user_token.expose()],
            },
            relying_party: "rp://api.minecraftservices.com/",
            token_type: "JWT",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxReply {
    token: Secret,
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<UserClaims>,
}

#[derive(Deserialize)]
struct UserClaims {
    uhs: String,
}

/// An Xbox Live token and the user hash that goes with it.
struct XboxToken {
    token: Secret,
    user_hash: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MinecraftLogin {
    identity_token: Secret,
}

impl MinecraftLogin {
    fn new(xsts: &XboxToken) -> Self {
        let identity = format!("XBL3.0 x={};{}", xsts.user_hash, xsts.token.expose());
        Self {
            identity_token: Secret::new(identity),
        }
    }
}

#[derive(Deserialize)]
struct MinecraftToken {
    access_token: Secret,
    expires_in: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_are_short_and_skip_trace_ids() {
        let body: ErrorBody = serde_json::from_str(
            r#"{"error":"invalid_grant","error_description":"AADSTS70000: The grant is invalid.\r\nTrace ID: 1\r\nCorrelation ID: 2"}"#,
        )
        .unwrap();
        assert_eq!(
            body.reason().as_deref(),
            Some("invalid_grant: AADSTS70000: The grant is invalid.")
        );

        let long = format!("{}é", "a".repeat(200));
        let cut = first_line(&long);
        assert_eq!(cut.chars().count(), 161);
        assert!(cut.ends_with('…'));

        let empty: ErrorBody = serde_json::from_str(r#"{"errorMessage":"","error":""}"#).unwrap();
        assert_eq!(empty.reason(), None);
    }

    #[test]
    fn lifetimes_saturate() {
        assert_eq!(from_now(u64::MAX), Timestamp::MAX);
    }
}
