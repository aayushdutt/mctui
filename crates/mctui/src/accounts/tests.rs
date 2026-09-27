//! Accounts and sign-in end to end: the whole app, with a mock standing in
//! for Microsoft, Xbox Live and Minecraft.

use std::time::Duration;

use crossterm::event::{Event as TermEvent, KeyCode, MouseButton, MouseEventKind};
use mctui_core::account::{Account, AccountKind, Accounts, Secret};
use wiremock::matchers::{bearer_token, body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{SessionStatus, offline};
use crate::app::tests::Harness;
use crate::data::Data;
use crate::task::Event;
use crate::ui::Target;

const DEVICE_CODE: &str = r#"{"user_code":"F7K9ZQ2X","device_code":"device-code-secret",
    "verification_uri":"https://www.microsoft.com/link","expires_in":900,"interval":5}"#;
const TOKENS: &str = r#"{"access_token":"ms-access-token","refresh_token":"ms-refresh-token",
    "token_type":"Bearer","expires_in":3600}"#;
const XBOX: &str = r#"{"Token":"xbl-token","DisplayClaims":{"xui":[{"uhs":"4312"}]}}"#;
const MINECRAFT: &str = r#"{"access_token":"mc-access-token","expires_in":86400}"#;
/// Who signs in: jeb_.
const JEB: &str = r#"{"id":"853c80ef3c3749fdaa49938b674adae6","name":"jeb_"}"#;
const JEB_ID: &str = "853c80ef3c3749fdaa49938b674adae6";
/// The fixture's active account, as Minecraft knows him.
const STEVE: &str = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Steve"}"#;
const STEVE_ID: &str = "069a79f444e94726a5befca90e38aaf5";

fn json(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(body.to_owned(), "application/json")
}

/// Microsoft's answer to a poll that isn't a sign-in (yet).
fn oauth_error(code: &str) -> ResponseTemplate {
    let body = format!(
        r#"{{"error":"{code}","error_description":"AADSTS70000: Details.\r\nTrace ID: 1"}}"#
    );
    json(400, &body)
}

/// Microsoft hands out a code, then answers polls with `polls` in turn, the
/// last one for good.
fn given_code(h: &Harness, polls: Vec<ResponseTemplate>) {
    h.mock(
        Mock::given(method("POST"))
            .and(path("/devicecode"))
            .respond_with(json(200, DEVICE_CODE)),
    );
    let last = polls.len().saturating_sub(1);
    for (i, answer) in polls.into_iter().enumerate() {
        let mock = Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("device_code=device-code-secret"))
            .respond_with(answer);
        h.mock(if i < last {
            mock.up_to_n_times(1)
        } else {
            mock
        });
    }
}

/// The player finishes signing in on Microsoft's page.
fn player_signs_in(h: &Harness) {
    h.mock(
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(json(200, TOKENS))
            .with_priority(1),
    );
}

/// Xbox Live and Minecraft take Microsoft's token to `profile`.
fn given_chain(h: &Harness, profile: &str) {
    for route in ["/user/authenticate", "/xsts/authorize"] {
        h.mock(
            Mock::given(method("POST"))
                .and(path(route))
                .respond_with(json(200, XBOX)),
        );
    }
    h.mock(
        Mock::given(method("POST"))
            .and(path("/authentication/login_with_xbox"))
            .respond_with(json(200, MINECRAFT)),
    );
    h.mock(
        Mock::given(method("GET"))
            .and(path("/minecraft/profile"))
            .and(bearer_token("mc-access-token"))
            .respond_with(json(200, profile)),
    );
}

/// Minecraft answers a check of the fixture's saved token with `answer`.
fn given_saved_session(h: &Harness, answer: ResponseTemplate) {
    h.mock(
        Mock::given(method("GET"))
            .and(path("/minecraft/profile"))
            .and(bearer_token("test-token"))
            .respond_with(answer),
    );
}

fn msa(id: &str, name: &str) -> Account {
    Account {
        id: id.to_owned(),
        name: name.to_owned(),
        kind: AccountKind::Msa,
        access_token: Secret::new("jeb-token"),
        expires_at: Some(jiff::Timestamp::now() + jiff::SignedDuration::from_hours(24)),
        msa_refresh_token: None,
    }
}

/// Steve (active), jeb_ (whose sign-in expired) and an offline Notch.
fn three_accounts(data: &mut Data) {
    data.accounts.upsert(msa(JEB_ID, "jeb_"));
    data.accounts.upsert(offline::account("Notch"));
    data.sessions
        .insert(JEB_ID.to_owned(), SessionStatus::Expired);
}

fn no_accounts(data: &mut Data) {
    data.accounts = Accounts::default();
}

fn active(h: &Harness) -> Option<&str> {
    h.data().accounts.active().map(|a| a.name.as_str())
}

/// What's in `accounts.json` now.
fn saved(h: &Harness) -> Accounts {
    Accounts::load(&h.data().dir).value
}

fn row(id: &str) -> Target {
    Target::Row {
        list: "accounts",
        key: id.to_owned(),
    }
}

#[test]
fn lists_accounts_with_their_state() {
    let mut h = Harness::with(80, 24, three_accounts);
    h.press(KeyCode::Char('a'));
    insta::assert_snapshot!(h.text());
}

#[test]
fn an_empty_list_offers_to_sign_in() {
    let mut h = Harness::with(80, 24, no_accounts);
    h.press(KeyCode::Char('a'));
    insta::assert_snapshot!(h.text());
    h.click(&Target::Key(KeyCode::Char('o')));
    assert!(h.text().contains("Offline profile"), "the button works");
}

#[test]
fn enter_switches_the_active_account_and_checks_it() {
    let mut h = Harness::with(80, 24, three_accounts);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    assert_eq!(active(&h), Some("jeb_"));
    assert_eq!(h.banner(), Some("Now playing as jeb_."));
    assert_eq!(saved(&h).active_id.as_deref(), Some(JEB_ID), "saved");
    assert!(h.text().contains("○ jeb_ · checking…"));
    // Nothing online answers here.
    h.settle();
    let text = h.text();
    assert!(text.contains("▲ jeb_ · can't verify"), "{text}");
}

#[test]
fn mouse_selects_activates_and_offers_actions() {
    let mut h = Harness::with(80, 24, three_accounts);
    h.press(KeyCode::Char('a'));
    let notch = offline::account("Notch").id;
    h.click(&row(&notch));
    assert_eq!(active(&h), Some("Steve"), "a click only selects");
    let at = h.find(&row(&notch));
    h.mouse(MouseEventKind::Down(MouseButton::Left), at);
    assert_eq!(active(&h), Some("Notch"), "a double-click plays as it");

    let at = h.find(&row(JEB_ID));
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    insta::assert_snapshot!(h.text());
    h.press(KeyCode::Char('d'));
    assert!(h.text().contains("Remove “jeb_” from mctui?"));
    h.press(KeyCode::Char('y'));
    assert_eq!(h.data().accounts.accounts.len(), 2);
    assert_eq!(h.banner(), Some("Removed “jeb_”."));
}

#[test]
fn removing_asks_first_and_opens_on_cancel() {
    let mut h = Harness::with(80, 24, three_accounts);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('d'));
    insta::assert_snapshot!(h.text());
    h.press(KeyCode::Enter);
    assert_eq!(h.data().accounts.accounts.len(), 3, "enter keeps it");

    // Removing the active account moves on to the next one.
    h.press(KeyCode::Char('d'));
    h.press(KeyCode::Left);
    h.press(KeyCode::Enter);
    assert_eq!(h.banner(), Some("Removed “Steve”."));
    assert_eq!(active(&h), Some("jeb_"));
    let on_disk = saved(&h);
    assert!(on_disk.accounts.iter().all(|a| a.id != STEVE_ID));
    assert_eq!(on_disk.active_id.as_deref(), Some(JEB_ID));

    // …and the last one leaves nobody signed in.
    h.press(KeyCode::Delete);
    h.press(KeyCode::Char('y'));
    h.press(KeyCode::Backspace);
    h.press(KeyCode::Char('y'));
    assert_eq!(active(&h), None);
    assert!(h.text().contains("Not signed in"));
}

#[test]
fn offline_profiles_need_a_valid_unused_name() {
    let mut h = Harness::new(80, 24);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('o'));
    h.type_text("ab");
    h.press(KeyCode::Enter);
    insta::assert_snapshot!(h.text());

    h.type_text("_2?");
    h.press(KeyCode::Enter);
    assert!(h.text().contains("Only letters, numbers and underscores."));
    h.press(KeyCode::Backspace);
    h.press(KeyCode::Enter);
    assert_eq!(h.banner(), Some("Added the offline profile “ab_2”."));
    assert!(h.text().contains("ACCOUNTS"), "back on the list");
    assert_eq!(active(&h), Some("Steve"), "the active account stays");
    let added = saved(&h)
        .accounts
        .into_iter()
        .find(|a| a.name == "ab_2")
        .expect("saved");
    assert_eq!(added.kind, AccountKind::Offline);
    assert_eq!(added.id, offline::account("ab_2").id);

    h.press(KeyCode::Char('o'));
    h.type_text("AB_2");
    h.press(KeyCode::Enter);
    assert!(
        h.text()
            .contains("There's already an offline profile called AB_2.")
    );
    h.press(KeyCode::Esc);
    assert!(h.text().contains("Discard this profile?"), "asks first");
}

#[test]
fn signs_in_with_microsoft() {
    let mut h = Harness::online(80, 24, |_| {});
    given_code(&h, vec![oauth_error("authorization_pending")]);
    given_chain(&h, JEB);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('n'));
    assert!(h.text().contains("Contacting Microsoft…"));

    h.settle();
    insta::assert_snapshot!(h.text());
    assert_eq!(
        h.app.take_clipboard().as_deref(),
        Some("F7K9ZQ2X"),
        "copied as soon as it appears"
    );
    assert!(h.requests("/token") > 0, "polling");

    player_signs_in(&h);
    h.settle();
    assert_eq!(h.banner(), Some("Signed in as jeb_."));
    assert_eq!(active(&h), Some("jeb_"), "a new sign-in becomes active");
    assert_eq!(saved(&h).active_id.as_deref(), Some(JEB_ID), "saved");
    let text = h.text();
    assert!(
        text.contains("ACCOUNTS") && text.contains("● jeb_"),
        "{text}"
    );
}

#[test]
fn copy_and_browser_buttons() {
    let mut h = Harness::online(80, 24, |_| {});
    given_code(&h, vec![oauth_error("authorization_pending")]);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('n'));
    h.settle();
    h.app.take_clipboard();
    h.click(&Target::Key(KeyCode::Char('c')));
    assert_eq!(h.app.take_clipboard().as_deref(), Some("F7K9ZQ2X"));
    h.click(&Target::Key(KeyCode::Char('b')));
    assert_eq!(h.banner(), None, "opened without a fuss");
}

#[test]
fn esc_cancels_a_sign_in_in_progress() {
    let mut h = Harness::online(80, 24, no_accounts);
    given_code(&h, vec![oauth_error("authorization_pending")]);
    given_chain(&h, JEB);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('n'));
    h.settle();
    h.press(KeyCode::Esc);
    assert!(h.text().contains("ACCOUNTS"), "back on the list");

    std::thread::sleep(Duration::from_millis(30));
    let polls = h.requests("/token");
    player_signs_in(&h);
    std::thread::sleep(Duration::from_millis(100));
    h.settle();
    assert_eq!(h.requests("/token"), polls, "stopped polling");
    assert!(h.data().accounts.accounts.is_empty(), "nothing added later");
    assert_eq!(h.banner(), None);
}

#[test]
fn sign_in_errors_say_what_to_do() {
    type Given = fn(&Harness);
    let cases: [(&str, Given); 6] = [
        ("The code expired before you finished signing in.", |h| {
            given_code(h, vec![oauth_error("expired_token")]);
        }),
        ("Sign-in was declined on Microsoft's page.", |h| {
            given_code(h, vec![oauth_error("authorization_declined")]);
        }),
        ("This Microsoft account has no Xbox profile yet", |h| {
            given_code(h, vec![json(200, TOKENS)]);
            h.mock(
                Mock::given(path("/xsts/authorize"))
                    .respond_with(json(401, r#"{"XErr":2148916233}"#))
                    .with_priority(1),
            );
            given_chain(h, JEB);
        }),
        (
            "This Microsoft account doesn't own Minecraft Java Edition.",
            |h| {
                given_code(h, vec![json(200, TOKENS)]);
                h.mock(
                    Mock::given(path("/minecraft/profile"))
                        .respond_with(json(404, r#"{"error":"NOT_FOUND"}"#))
                        .with_priority(1),
                );
                given_chain(h, JEB);
            },
        ),
        ("Microsoft couldn't sign you in right now", |h| {
            h.mock(
                Mock::given(path("/devicecode")).respond_with(oauth_error("unauthorized_client")),
            );
        }),
        ("Minecraft couldn't sign you in right now", |h| {
            given_code(h, vec![json(200, TOKENS)]);
            h.mock(
                Mock::given(path("/authentication/login_with_xbox"))
                    .respond_with(json(403, r#"{"errorMessage":"Invalid app registration"}"#))
                    .with_priority(1),
            );
            given_chain(h, JEB);
        }),
    ];
    for (expected, given) in cases {
        let mut h = Harness::online(100, 30, |_| {});
        given(&h);
        h.press(KeyCode::Char('a'));
        h.press(KeyCode::Char('n'));
        h.settle();
        let text = h.text();
        assert!(
            text.contains("Couldn't sign in") && text.contains(expected),
            "{expected}: {text}"
        );
        h.press(KeyCode::Char('r'));
        assert!(
            h.text().contains("Contacting Microsoft…"),
            "{expected}: r tries again"
        );
    }

    // With no connection at all.
    let mut h = Harness::new(100, 30);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('n'));
    h.settle();
    insta::assert_snapshot!(h.text());
    assert!(
        h.text()
            .contains("Couldn't reach Microsoft — check your connection, then press r.")
    );
}

#[test]
fn the_startup_check_updates_the_header() {
    // Minecraft accepts the saved token.
    let mut h = Harness::online(80, 24, |_| {});
    given_saved_session(&h, json(200, STEVE));
    h.app.start();
    assert!(h.text().contains("○ Steve · checking…"));
    h.settle();
    assert!(h.text().contains("● Steve"));
    assert_eq!(h.requests("/token"), 0, "no refresh needed");

    // Minecraft rejects it, and there's no saved sign-in to renew it with…
    let mut h = Harness::online(80, 24, |_| {});
    given_saved_session(&h, ResponseTemplate::new(401));
    h.app.start();
    h.settle();
    let text = h.text();
    assert!(text.contains("▲ Steve · sign-in expired"), "{text}");
    assert!(text.contains("Sign in & play"), "{text}");

    // Offline, it can't tell either way: that's not a reason to sign in.
    let mut h = Harness::new(80, 24);
    h.app.start();
    h.settle();
    let text = h.text();
    assert!(text.contains("▲ Steve · can't verify"), "{text}");
    assert!(!text.contains("Sign in & play"), "{text}");
}

#[test]
fn a_renewed_session_replaces_the_saved_one() {
    let mut h = Harness::online(80, 24, |d| {
        let steve = &mut d.accounts.accounts[0];
        steve.expires_at = None;
        steve.msa_refresh_token = Some(Secret::new("old-refresh-token"));
    });
    h.mock(
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("refresh_token=old-refresh-token"))
            .respond_with(json(200, TOKENS)),
    );
    given_chain(&h, STEVE);
    h.app.start();
    h.settle();
    assert!(h.text().contains("● Steve"));
    let steve = saved(&h).accounts.into_iter().next().expect("saved");
    assert_eq!(steve.access_token.expose(), "mc-access-token");
    assert_eq!(
        steve.msa_refresh_token.as_ref().map(Secret::expose),
        Some("ms-refresh-token")
    );
}

/// A-11: one check per account at a time.
#[test]
fn checks_never_overlap() {
    let mut h = Harness::online(80, 24, |_| {});
    given_saved_session(&h, json(200, STEVE).set_delay(Duration::from_millis(100)));
    h.app.start();
    h.app.start();
    h.settle();
    assert_eq!(h.requests("/minecraft/profile"), 1);
}

/// A-8: Play without an account signs in, then plays.
#[test]
fn play_signs_in_first_then_launches() {
    let mut h = Harness::online(80, 24, no_accounts);
    given_code(&h, vec![json(200, TOKENS)]);
    given_chain(&h, JEB);
    assert!(h.text().contains("Sign in & play"));
    h.press(KeyCode::Enter);
    let text = h.text();
    assert!(
        text.contains("Sign in with Microsoft to play Survival 1.21 online."),
        "{text}"
    );

    h.settle();
    assert_eq!(active(&h), Some("jeb_"));
    let launching = h
        .data()
        .game
        .as_ref()
        .map(|g| (g.instance_id.as_str(), g.offline));
    assert_eq!(
        launching,
        Some(("Survival 1.21", false)),
        "the launch carried on"
    );
}

#[test]
fn a_player_who_cant_sign_in_can_play_offline() {
    let mut h = Harness::with(80, 24, no_accounts);
    h.press(KeyCode::Enter);
    h.settle();
    assert!(h.text().contains("Couldn't reach Microsoft"));
    h.press(KeyCode::Char('o'));
    let launching = h.data().game.as_ref().map(|g| g.offline);
    assert_eq!(launching, Some(true), "launching offline");
    assert!(h.text().contains("Getting ready"), "on the launch screen");
}

#[test]
fn saving_problems_are_shown() {
    let mut h = Harness::with(80, 24, three_accounts);
    // A folder where accounts.json goes: writing it fails.
    std::fs::create_dir_all(h.data().dir.accounts_file()).expect("mkdir");
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    let banner = h.banner().unwrap_or_default();
    assert!(
        banner.starts_with("Now playing as jeb_. Not saved — "),
        "{banner}"
    );
    assert_eq!(active(&h), Some("jeb_"), "still switched for this run");
}

/// An `accounts.json` that couldn't be read at startup is never written over.
#[cfg(unix)]
#[test]
fn an_unreadable_accounts_file_is_left_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let mut h = Harness::with(80, 24, |data| {
        let path = data.dir.accounts_file();
        std::fs::write(&path, "{}").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        data.accounts = Accounts::load(&data.dir).value;
    });
    if std::fs::read(h.data().dir.accounts_file()).is_ok() {
        return; // running as root: permissions don't apply
    }
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('o'));
    h.type_text("Notch");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.banner(),
        Some(
            "Added the offline profile “Notch”. Not saved — accounts.json couldn't be read, \
             so it's left untouched."
        )
    );
}

#[test]
fn never_panics_at_any_size() {
    let mut h = Harness::online(100, 30, three_accounts);
    given_code(&h, vec![oauth_error("authorization_pending")]);
    let every_size = |h: &mut Harness| {
        for w in (0..=100).step_by(9) {
            for hgt in (0..=30).step_by(4) {
                h.app.handle(Event::Term(TermEvent::Resize(w, hgt)));
                h.draw();
            }
        }
        h.app.handle(Event::Term(TermEvent::Resize(100, 30)));
    };
    h.press(KeyCode::Char('a'));
    every_size(&mut h);
    h.press(KeyCode::Char('n'));
    h.settle();
    assert!(h.text().contains("F 7 K 9 Z Q 2 X"));
    every_size(&mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('o'));
    every_size(&mut h);
}

/// Renders the accounts and sign-in screens for `app::tests::screenshots`;
/// `light` switches a harness to the light theme.
pub(crate) fn screenshots(save: &dyn Fn(&str, &mut Harness), light: &dyn Fn(&mut Harness)) {
    let mut h = Harness::with(100, 30, three_accounts);
    h.press(KeyCode::Char('a'));
    save("accounts", &mut h);
    light(&mut h);
    save("accounts-light", &mut h);
    let mut h = Harness::with(100, 30, three_accounts);
    h.press(KeyCode::Char('a'));
    let at = h.find(&row(JEB_ID));
    h.mouse(MouseEventKind::Moved, at);
    save("accounts-hover", &mut h);
    h.mouse(MouseEventKind::Down(MouseButton::Right), at);
    save("accounts-menu", &mut h);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('d'));
    save("accounts-remove", &mut h);

    let mut h = Harness::with(100, 30, no_accounts);
    h.press(KeyCode::Char('a'));
    save("accounts-empty", &mut h);

    let mut h = Harness::new(100, 30);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('o'));
    h.type_text("ab");
    h.press(KeyCode::Enter);
    save("offline-profile", &mut h);

    let mut h = Harness::online(100, 30, |_| {});
    given_code(&h, vec![oauth_error("authorization_pending")]);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('n'));
    save("sign-in-contacting", &mut h);
    h.settle();
    save("sign-in-code", &mut h);
    light(&mut h);
    save("sign-in-code-light", &mut h);

    let mut h = Harness::online(100, 30, no_accounts);
    given_code(&h, vec![oauth_error("authorization_pending")]);
    h.press(KeyCode::Enter);
    h.settle();
    save("sign-in-play", &mut h);

    let mut h = Harness::online(100, 30, |_| {});
    given_code(&h, vec![json(200, TOKENS)]);
    h.mock(
        Mock::given(path("/minecraft/profile"))
            .respond_with(json(404, r#"{"error":"NOT_FOUND"}"#))
            .with_priority(1),
    );
    given_chain(&h, JEB);
    h.press(KeyCode::Char('a'));
    h.press(KeyCode::Char('n'));
    h.settle();
    save("sign-in-error", &mut h);

    let mut h = Harness::online(100, 30, |_| {});
    given_saved_session(&h, ResponseTemplate::new(401));
    h.app.start();
    save("header-checking", &mut h);
    h.settle();
    save("header-expired", &mut h);
}
