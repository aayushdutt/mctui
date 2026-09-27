//! Microsoft sign-in against a mock of every service it talks to, plus one
//! ignored check against Microsoft itself. The success fixtures under
//! `tests/fixtures/auth` mirror real answers, with placeholder tokens.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::time::{Duration, Instant};

use jiff::{SignedDuration, Timestamp};
use mctui_core::account::{Account, AccountKind, Secret};
use mctui_core::auth::{
    Auth, DeviceCode, Endpoints, Error, MsTokens, Profile, Session, XboxRefusal,
};
use mctui_core::config::DEFAULT_MSA_CLIENT_ID;
use mctui_core::net::Http;
use reqwest::StatusCode;
use serde_json::{Value, json};
use wiremock::matchers::{bearer_token, body_json, body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CLIENT_ID: &str = "test-client";
const USER_HASH: &str = "4312876543219876543";
const PROFILE_ID: &str = "069a79f444e94726a5befca90e38aaf5";
/// Every token the fixtures and tests use. None may ever be shown.
const TOKENS: [&str; 8] = [
    "device-code-secret",
    "ms-access-token",
    "ms-refresh-token",
    "xbl-token",
    "xsts-token",
    "mc-access-token",
    "stale-mc-token",
    "old-refresh-token",
];

fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/auth")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn reply(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

/// Microsoft's OAuth error answer, trace ids and all.
fn oauth_error(status: u16, error: &str) -> ResponseTemplate {
    reply(
        status,
        json!({
            "error": error,
            "error_description": "AADSTS70000: Details for the player.\r\nTrace ID: 0f1e\r\nCorrelation ID: 2d3c",
            "error_codes": [70000],
        }),
    )
}

/// A mock of every service, and an `Auth` that uses it and polls in
/// milliseconds.
async fn services() -> (MockServer, Auth) {
    let server = MockServer::start().await;
    let auth = Auth::with_endpoints(Http::new().unwrap(), endpoints(&server.uri()))
        .with_poll_unit(Duration::from_millis(1));
    (server, auth)
}

fn endpoints(base: &str) -> Endpoints {
    Endpoints {
        device_code: format!("{base}/devicecode"),
        token: format!("{base}/token"),
        xbox_user: format!("{base}/user/authenticate"),
        xsts: format!("{base}/xsts/authorize"),
        minecraft_login: format!("{base}/authentication/login_with_xbox"),
        minecraft_profile: format!("{base}/minecraft/profile"),
    }
}

/// A base URL nothing listens on.
fn unreachable() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

/// The device-code endpoint answers a correct request with `body`.
async fn given_device_code(server: &MockServer, body: Value) {
    Mock::given(method("POST"))
        .and(path("/devicecode"))
        .and(body_string_contains(format!("client_id={CLIENT_ID}")))
        .and(body_string_contains("scope=XboxLive.signin+offline_access"))
        .respond_with(reply(200, body))
        .mount(server)
        .await;
}

/// The token endpoint answers polls for the fixture's code with each of
/// `answers` in turn.
async fn given_polls(server: &MockServer, answers: Vec<ResponseTemplate>) {
    for answer in answers {
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains(
                "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
            ))
            .and(body_string_contains(format!("client_id={CLIENT_ID}")))
            .and(body_string_contains("device_code=device-code-secret"))
            .respond_with(answer)
            .up_to_n_times(1)
            .mount(server)
            .await;
    }
}

/// The token endpoint answers a refresh with the saved refresh token.
async fn given_refresh(server: &MockServer, answer: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains(format!("client_id={CLIENT_ID}")))
        .and(body_string_contains("refresh_token=old-refresh-token"))
        .respond_with(answer)
        .mount(server)
        .await;
}

/// Xbox Live, XSTS, Minecraft and its profile, each answering only a request
/// that carries the previous step's token.
async fn given_chain(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/user/authenticate"))
        .and(body_json(json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": "d=ms-access-token",
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        })))
        .respond_with(reply(200, fixture("xbl.json")))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/xsts/authorize"))
        .and(body_json(json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": ["xbl-token"] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        })))
        .respond_with(reply(200, fixture("xsts.json")))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/authentication/login_with_xbox"))
        .and(body_json(
            json!({ "identityToken": format!("XBL3.0 x={USER_HASH};xsts-token") }),
        ))
        .respond_with(reply(200, fixture("login_with_xbox.json")))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/minecraft/profile"))
        .and(bearer_token("mc-access-token"))
        .respond_with(reply(200, fixture("profile.json")))
        .mount(server)
        .await;
}

/// The saved account's profile check answers with `answer`.
async fn given_saved_profile(server: &MockServer, answer: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/minecraft/profile"))
        .and(bearer_token("stale-mc-token"))
        .respond_with(answer)
        .mount(server)
        .await;
}

/// Polls, failing the test rather than waiting out a 15-minute code.
async fn poll(auth: &Auth, code: &DeviceCode) -> Result<MsTokens, Error> {
    tokio::time::timeout(Duration::from_secs(10), auth.poll(code))
        .await
        .expect("poll never finished")
}

/// Signs in as far as Microsoft's tokens.
async fn ms_tokens(server: &MockServer, auth: &Auth) -> MsTokens {
    given_device_code(server, fixture("devicecode.json")).await;
    given_polls(server, vec![reply(200, fixture("token.json"))]).await;
    let code = auth.start(CLIENT_ID).await.unwrap();
    poll(auth, &code).await.unwrap()
}

/// A saved account whose Minecraft token has `left` before it expires.
fn saved(left: SignedDuration) -> Account {
    Account {
        id: PROFILE_ID.to_owned(),
        name: "Notch".to_owned(),
        kind: AccountKind::Msa,
        access_token: Secret::new("stale-mc-token"),
        expires_at: Some(Timestamp::now() + left),
        msa_refresh_token: Some(Secret::new("old-refresh-token")),
    }
}

async fn requests_to(server: &MockServer, route: &str) -> usize {
    let requests = server.received_requests().await.unwrap_or_default();
    requests.iter().filter(|r| r.url.path() == route).count()
}

#[tokio::test]
async fn signs_in_from_code_to_account() {
    let (server, auth) = services().await;
    given_chain(&server).await;
    given_device_code(&server, fixture("devicecode.json")).await;
    given_polls(&server, vec![reply(200, fixture("token.json"))]).await;

    let code = auth.start(CLIENT_ID).await.unwrap();
    assert_eq!(code.user_code, "F7K9ZQ2X");
    assert_eq!(code.verification_uri, "https://www.microsoft.com/link");
    assert_eq!(code.interval, Duration::from_millis(5));
    let lifetime = Timestamp::now().duration_until(code.expires_at);
    assert!(
        lifetime > SignedDuration::from_mins(14) && lifetime <= SignedDuration::from_mins(15),
        "{lifetime:?}"
    );

    let account = auth
        .finish(poll(&auth, &code).await.unwrap())
        .await
        .unwrap();
    assert_eq!(account.kind, AccountKind::Msa);
    assert_eq!(account.id, PROFILE_ID);
    assert_eq!(account.name, "Notch");
    assert_eq!(
        account.access_token.expose(),
        "mc-access-token",
        "Minecraft's token, not Microsoft's"
    );
    assert_eq!(
        account.msa_refresh_token.as_ref().map(Secret::expose),
        Some("ms-refresh-token")
    );
    let lifetime = Timestamp::now().duration_until(account.expires_at.unwrap());
    assert!(
        lifetime > SignedDuration::from_hours(23) && lifetime <= SignedDuration::from_hours(24),
        "{lifetime:?}"
    );
}

#[tokio::test]
async fn keeps_polling_while_the_player_signs_in() {
    let (server, auth) = services().await;
    given_device_code(&server, fixture("devicecode.json")).await;
    let pending = || oauth_error(400, "authorization_pending");
    given_polls(
        &server,
        vec![pending(), pending(), reply(200, fixture("token.json"))],
    )
    .await;

    let code = auth.start(CLIENT_ID).await.unwrap();
    poll(&auth, &code).await.unwrap();
    assert_eq!(requests_to(&server, "/token").await, 3);
}

#[tokio::test]
async fn slows_down_when_asked() {
    let (server, auth) = services().await;
    let auth = auth.with_poll_unit(Duration::from_millis(10));
    given_device_code(&server, fixture("devicecode.json")).await;
    given_polls(
        &server,
        vec![
            oauth_error(400, "slow_down"),
            reply(200, fixture("token.json")),
        ],
    )
    .await;

    let code = auth.start(CLIENT_ID).await.unwrap();
    let started = Instant::now();
    poll(&auth, &code).await.unwrap();
    // 50 ms before the first poll, then 50 + 50 before the second.
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(150), "{elapsed:?}");
}

/// A-4: an outage, a captive portal's page, or an empty token isn't a sign-in.
#[tokio::test]
async fn outages_and_garbage_are_not_a_sign_in() {
    let (server, auth) = services().await;
    given_chain(&server).await;
    given_device_code(&server, fixture("devicecode.json")).await;
    given_polls(
        &server,
        vec![
            ResponseTemplate::new(503).set_body_string("<html>Service Unavailable</html>"),
            ResponseTemplate::new(200).set_body_string("<html>Sign in to the café wifi</html>"),
            reply(200, json!({ "access_token": "", "token_type": "Bearer" })),
            reply(200, fixture("token.json")),
        ],
    )
    .await;

    let code = auth.start(CLIENT_ID).await.unwrap();
    let tokens = poll(&auth, &code).await.unwrap();
    assert_eq!(requests_to(&server, "/token").await, 4);
    // The chain only answers Microsoft's real token.
    auth.finish(tokens).await.unwrap();
}

#[tokio::test]
async fn an_expired_code_ends_the_wait() {
    let (server, auth) = services().await;
    given_device_code(&server, fixture("devicecode.json")).await;
    given_polls(&server, vec![oauth_error(400, "expired_token")]).await;

    let code = auth.start(CLIENT_ID).await.unwrap();
    let err = poll(&auth, &code).await.unwrap_err();
    assert!(matches!(err, Error::Expired), "{err:?}");
    assert_eq!(
        err.to_string(),
        "the sign-in code expired — start again for a new one"
    );
}

/// A-1: the wait lasts as long as the code (15 minutes above), and no longer.
#[tokio::test]
async fn gives_up_when_the_code_expires() {
    let (server, auth) = services().await;
    let mut body = fixture("devicecode.json");
    body["expires_in"] = json!(0);
    given_device_code(&server, body).await;

    let code = auth.start(CLIENT_ID).await.unwrap();
    assert!(matches!(poll(&auth, &code).await, Err(Error::Expired)));
    assert_eq!(requests_to(&server, "/token").await, 0);
}

#[tokio::test]
async fn declining_ends_the_wait() {
    for error in ["authorization_declined", "access_denied"] {
        let (server, auth) = services().await;
        given_device_code(&server, fixture("devicecode.json")).await;
        given_polls(&server, vec![oauth_error(400, error)]).await;

        let code = auth.start(CLIENT_ID).await.unwrap();
        let err = poll(&auth, &code).await.unwrap_err();
        assert!(matches!(err, Error::Denied), "{error}: {err:?}");
    }
}

#[tokio::test]
async fn unexpected_poll_errors_say_why_without_trace_ids() {
    let (server, auth) = services().await;
    given_device_code(&server, fixture("devicecode.json")).await;
    given_polls(&server, vec![oauth_error(400, "bad_verification_code")]).await;

    let code = auth.start(CLIENT_ID).await.unwrap();
    let err = poll(&auth, &code).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "Microsoft answered 400 Bad Request: bad_verification_code: AADSTS70000: Details for the player."
    );
}

#[tokio::test]
async fn start_reports_a_refused_client_and_a_dead_network() {
    let (server, auth) = services().await;
    Mock::given(method("POST"))
        .and(path("/devicecode"))
        .respond_with(oauth_error(400, "unauthorized_client"))
        .mount(&server)
        .await;
    let err = auth.start(CLIENT_ID).await.unwrap_err();
    assert!(
        matches!(
            err,
            Error::Service {
                status: StatusCode::BAD_REQUEST,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(err.to_string().contains("unauthorized_client"), "{err}");

    let offline = Auth::with_endpoints(Http::new().unwrap(), endpoints(&unreachable()));
    let err = offline.start(CLIENT_ID).await.unwrap_err();
    assert!(matches!(err, Error::Network { .. }), "{err:?}");
    assert_eq!(
        err.to_string(),
        "couldn't reach Microsoft — check your connection"
    );
}

/// A-7: Xbox Live's refusals are explained, not dumped as JSON.
#[tokio::test]
async fn xbox_refusals_are_explained() {
    let cases = [
        (2_148_916_233_u64, XboxRefusal::NoAccount),
        (2_148_916_235, XboxRefusal::Region),
        (2_148_916_236, XboxRefusal::AdultVerification),
        (2_148_916_237, XboxRefusal::AgeVerification),
        (2_148_916_238, XboxRefusal::Child),
        (2_148_916_227, XboxRefusal::Other(2_148_916_227)),
    ];
    for (code, refusal) in cases {
        let (server, auth) = services().await;
        let mut body = fixture("xsts_refused.json");
        body["XErr"] = json!(code);
        Mock::given(method("POST"))
            .and(path("/xsts/authorize"))
            .respond_with(reply(401, body))
            .with_priority(1)
            .mount(&server)
            .await;
        given_chain(&server).await;

        let tokens = ms_tokens(&server, &auth).await;
        let err = auth.finish(tokens).await.unwrap_err();
        assert!(
            matches!(err, Error::Xbox(r) if r == refusal),
            "{code}: {err:?}"
        );
        assert!(!err.to_string().contains('{'), "{err}");
    }
}

/// A-7: a missing profile means the game isn't owned.
#[tokio::test]
async fn a_missing_profile_means_no_minecraft() {
    let (server, auth) = services().await;
    Mock::given(method("GET"))
        .and(path("/minecraft/profile"))
        .respond_with(reply(404, fixture("profile_not_found.json")))
        .with_priority(1)
        .mount(&server)
        .await;
    given_chain(&server).await;

    let tokens = ms_tokens(&server, &auth).await;
    let err = auth.finish(tokens).await.unwrap_err();
    assert!(matches!(err, Error::NoMinecraft), "{err:?}");
}

#[tokio::test]
async fn minecraft_login_failures_say_why() {
    let (server, auth) = services().await;
    Mock::given(method("POST"))
        .and(path("/authentication/login_with_xbox"))
        .respond_with(reply(403, fixture("login_invalid_app.json")))
        .with_priority(1)
        .mount(&server)
        .await;
    given_chain(&server).await;

    let tokens = ms_tokens(&server, &auth).await;
    let err = auth.finish(tokens).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "Minecraft answered 403 Forbidden: Invalid app registration, see https://aka.ms/AppRegInfo for more information"
    );
}

#[tokio::test]
async fn refresh_saves_a_rotated_refresh_token() {
    let (server, auth) = services().await;
    given_refresh(&server, reply(200, fixture("token.json"))).await;
    given_chain(&server).await;

    let account = auth
        .refresh(&saved(SignedDuration::ZERO), CLIENT_ID)
        .await
        .unwrap();
    assert_eq!(account.id, PROFILE_ID);
    assert_eq!(account.access_token.expose(), "mc-access-token");
    assert_eq!(
        account.msa_refresh_token.as_ref().map(Secret::expose),
        Some("ms-refresh-token")
    );
}

/// A-5: OAuth needn't rotate the refresh token, and losing it would force the
/// player to sign in again.
#[tokio::test]
async fn refresh_keeps_a_refresh_token_that_was_not_rotated() {
    let (server, auth) = services().await;
    let mut body = fixture("token.json");
    body.as_object_mut().unwrap().remove("refresh_token");
    given_refresh(&server, reply(200, body)).await;
    given_chain(&server).await;

    let account = auth
        .refresh(&saved(SignedDuration::ZERO), CLIENT_ID)
        .await
        .unwrap();
    assert_eq!(
        account.msa_refresh_token.as_ref().map(Secret::expose),
        Some("old-refresh-token")
    );
}

/// A-6: only a dead refresh token means signing in again.
#[tokio::test]
async fn refresh_asks_for_sign_in_only_when_the_grant_is_gone() {
    for error in ["invalid_grant", "interaction_required", "consent_required"] {
        let (server, auth) = services().await;
        given_refresh(&server, oauth_error(400, error)).await;
        let err = auth
            .refresh(&saved(SignedDuration::ZERO), CLIENT_ID)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NeedsSignIn), "{error}: {err:?}");
    }

    let (server, auth) = services().await;
    let mut account = saved(SignedDuration::ZERO);
    account.msa_refresh_token = None;
    let err = auth.refresh(&account, CLIENT_ID).await.unwrap_err();
    assert!(matches!(err, Error::NeedsSignIn), "{err:?}");
    assert_eq!(requests_to(&server, "/token").await, 0);
}

/// A-6: a bad client id, an outage, rate limiting or a dead network aren't
/// fixed by signing in again.
#[tokio::test]
async fn refresh_failures_that_signing_in_would_not_fix() {
    let cases = [
        (oauth_error(401, "invalid_client"), StatusCode::UNAUTHORIZED),
        (
            ResponseTemplate::new(500).set_body_string("<html>oops</html>"),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (ResponseTemplate::new(429), StatusCode::TOO_MANY_REQUESTS),
        (
            oauth_error(503, "temporarily_unavailable"),
            StatusCode::SERVICE_UNAVAILABLE,
        ),
    ];
    for (answer, expected) in cases {
        let (server, auth) = services().await;
        given_refresh(&server, answer).await;
        let err = auth
            .refresh(&saved(SignedDuration::ZERO), CLIENT_ID)
            .await
            .unwrap_err();
        assert!(
            matches!(err, Error::Service { status, .. } if status == expected),
            "{expected}: {err:?}"
        );
    }

    let offline = Auth::with_endpoints(Http::new().unwrap(), endpoints(&unreachable()));
    let err = offline
        .refresh(&saved(SignedDuration::ZERO), CLIENT_ID)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Network { .. }), "{err:?}");
}

#[tokio::test]
async fn profile_checks_the_token() {
    let (server, auth) = services().await;
    given_saved_profile(&server, reply(200, fixture("profile.json"))).await;
    let profile = auth
        .profile(&saved(SignedDuration::from_hours(12)))
        .await
        .unwrap();
    let expected = Profile {
        id: PROFILE_ID.to_owned(),
        name: "Notch".to_owned(),
    };
    assert_eq!(profile, expected);

    let (server, auth) = services().await;
    given_saved_profile(&server, ResponseTemplate::new(401)).await;
    let err = auth
        .profile(&saved(SignedDuration::from_hours(12)))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::SessionExpired), "{err:?}");
}

#[tokio::test]
async fn a_working_session_is_left_alone() {
    let (server, auth) = services().await;
    given_saved_profile(&server, reply(200, fixture("profile.json"))).await;

    let account = saved(SignedDuration::from_hours(12));
    let session = auth
        .ensure_session(&account, CLIENT_ID, SignedDuration::from_mins(5))
        .await
        .unwrap();
    assert_eq!(session, Session::Valid);
    assert_eq!(requests_to(&server, "/token").await, 0);
}

/// A-10: a token Minecraft rejects is refreshed, though it hasn't expired.
#[tokio::test]
async fn a_rejected_session_is_refreshed() {
    let (server, auth) = services().await;
    given_saved_profile(&server, ResponseTemplate::new(401)).await;
    given_refresh(&server, reply(200, fixture("token.json"))).await;
    given_chain(&server).await;

    let account = saved(SignedDuration::from_hours(12));
    let session = auth
        .ensure_session(&account, CLIENT_ID, SignedDuration::from_mins(5))
        .await
        .unwrap();
    let Session::Refreshed(account) = session else {
        panic!("not refreshed: {session:?}");
    };
    assert_eq!(account.access_token.expose(), "mc-access-token");
}

/// A-10: a launch refreshes a token that would expire mid-game, without
/// asking Minecraft about the old one first.
#[tokio::test]
async fn a_session_about_to_expire_is_refreshed_up_front() {
    let (server, auth) = services().await;
    Mock::given(method("GET"))
        .and(bearer_token("stale-mc-token"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    given_refresh(&server, reply(200, fixture("token.json"))).await;
    given_chain(&server).await;

    let account = saved(SignedDuration::from_mins(30));
    let session = auth
        .ensure_session(&account, CLIENT_ID, SignedDuration::from_hours(4))
        .await
        .unwrap();
    assert!(matches!(session, Session::Refreshed(_)), "{session:?}");
}

#[tokio::test]
async fn tokens_never_show_in_debug_output() {
    let (server, auth) = services().await;
    given_chain(&server).await;
    given_device_code(&server, fixture("devicecode.json")).await;
    given_polls(&server, vec![reply(200, fixture("token.json"))]).await;

    let code = auth.start(CLIENT_ID).await.unwrap();
    let tokens = poll(&auth, &code).await.unwrap();
    let account = auth.finish(tokens.clone()).await.unwrap();
    let session = Session::Refreshed(saved(SignedDuration::ZERO));
    let shown = format!("{auth:?} {code:?} {tokens:?} {account:?} {session:?}");
    for token in TOKENS {
        assert!(!shown.contains(token), "{token} shown in {shown}");
    }
}

/// Asks Microsoft for a real code with the default client id, without
/// finishing the sign-in. Run with
/// `cargo test -p mctui-core --test auth -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "talks to Microsoft"]
async fn live_start_gets_a_code() {
    let auth = Auth::new(Http::new().unwrap());
    let code = auth.start(DEFAULT_MSA_CLIENT_ID).await.unwrap();
    println!(
        "enter {} at {} before {} (polling every {:?})",
        code.user_code, code.verification_uri, code.expires_at, code.interval
    );
    assert!(!code.user_code.is_empty());
    assert!(code.expires_at > Timestamp::now());
}
