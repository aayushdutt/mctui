//! Adding an offline profile: just a name, for singleplayer and LAN games.

use std::fmt::Write as _;

use crossterm::event::{KeyCode, KeyEvent};
use mctui_core::account::{Account, AccountKind, Secret};
use md5::{Digest as _, Md5};
use ratatui::layout::Rect;

use super::Change;
use crate::app::{Action, Question};
use crate::data::Data;
use crate::overlay::HelpGroup;
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::ui::form::{Control, Field, Form, FormEvent};
use crate::ui::{Canvas, Hint, Target};

const WIDTH: u16 = 76;

/// An offline profile named `name`.
pub fn account(name: &str) -> Account {
    Account {
        id: uuid(name),
        name: name.to_owned(),
        kind: AccountKind::Offline,
        access_token: Secret::default(),
        expires_at: None,
        msa_refresh_token: None,
    }
}

/// The UUID Minecraft gives an offline player (Java's
/// `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`: an MD5, version 3),
/// without dashes like the Microsoft ones.
fn uuid(name: &str) -> String {
    let mut bytes = Md5::digest(format!("OfflinePlayer:{name}"));
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes.iter().fold(String::with_capacity(32), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

/// What's wrong with `name` as a player name, by Minecraft's rule: 3 to 16
/// letters, digits and underscores.
fn name_problem(name: &str) -> Option<&'static str> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Some("Only letters, numbers and underscores.")
    } else if name.len() < 3 {
        Some("Too short: use at least 3 characters.")
    } else if name.len() > 16 {
        Some("Too long: use at most 16 characters.")
    } else {
        None
    }
}

#[derive(Debug)]
pub struct OfflineProfile {
    form: Form,
    body: Rect,
}

impl OfflineProfile {
    pub fn new() -> Self {
        let form = Form::new(vec![
            Field::new("name", "Name", Control::text("", "Steve")).help(
                "3–16 letters, numbers or underscores. Plays singleplayer and LAN, \
                 not online servers.",
            ),
        ]);
        Self {
            form,
            body: Rect::default(),
        }
    }

    fn on_form(&mut self, event: FormEvent, data: &Data) -> Handled {
        match event {
            FormEvent::Ignored => Handled::Ignored,
            FormEvent::Moved | FormEvent::Edited(_) => Handled::Consumed,
            FormEvent::Submit => self.add(data),
            FormEvent::Cancel if self.form.is_dirty() => {
                Handled::Action(Action::Confirm(Box::new(Question {
                    title: "Discard this profile?".into(),
                    message: "The offline profile hasn't been added.".into(),
                    yes: "Discard",
                    danger: None,
                    on_yes: Action::Close,
                })))
            }
            FormEvent::Cancel => Handled::Action(Action::Close),
        }
    }

    fn add(&mut self, data: &Data) -> Handled {
        let name = self.form.text("name").trim().to_owned();
        let taken = data
            .accounts
            .accounts
            .iter()
            .any(|a| a.kind == AccountKind::Offline && a.name.eq_ignore_ascii_case(&name));
        let error = if let Some(problem) = name_problem(&name) {
            problem.to_owned()
        } else if taken {
            format!("There's already an offline profile called {name}.")
        } else {
            return Handled::Action(Action::Many(vec![
                Action::Account(Change::AddOffline(name)),
                Action::Close,
            ]));
        };
        self.form.set_error("name", Some(error));
        Handled::Consumed
    }
}

impl Screen for OfflineProfile {
    fn crumbs(&self, _data: &Data) -> Vec<String> {
        vec!["Accounts".into(), "Offline profile".into()]
    }

    fn hints(&self, _data: &Data) -> Vec<Hint> {
        let esc = if self.form.is_dirty() {
            "discard"
        } else {
            "back"
        };
        vec![
            Hint::key(KeyCode::Enter, "add"),
            Hint::key(KeyCode::Esc, esc),
        ]
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        let groups = vec![
            HelpGroup {
                title: "Edit",
                entries: &[("type", "the player name")],
            },
            HelpGroup {
                title: "General",
                entries: &[("↵", "add"), ("esc", "back · discard"), ("?", "help")],
            },
        ];
        ("Offline profile", groups)
    }

    fn captures_text(&self) -> bool {
        self.form.captures_text()
    }

    fn set_body(&mut self, body: Rect) {
        self.body = body;
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        let event = self.form.handle_key(key);
        self.on_form(event, cx.data)
    }

    fn handle_mouse(&mut self, target: &Target, mouse: Mouse, cx: &mut Ctx<'_>) -> Handled {
        match (target, mouse) {
            (Target::Field(i), Mouse::Click { .. }) => {
                let event = self.form.click(*i);
                self.on_form(event, cx.data)
            }
            _ => Handled::Ignored,
        }
    }

    fn handle_paste(&mut self, text: &str, cx: &mut Ctx<'_>) -> Handled {
        let event = self.form.handle_paste(text);
        self.on_form(event, cx.data)
    }

    fn render(&self, cv: &mut Canvas<'_>, _data: &Data) {
        let area = Rect {
            x: self.body.x + 2,
            width: self.body.width.saturating_sub(4).min(WIDTH),
            ..self.body
        };
        self.form.render(cv, area, true, Target::Field);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_ids_match_minecraft() {
        // What vanilla servers in offline mode give Notch.
        assert_eq!(uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
        assert_eq!(account("Notch").kind, AccountKind::Offline);
    }

    #[test]
    fn names_follow_minecraft_rules() {
        for good in ["Abc", "jeb_", "Player_123456789"] {
            assert_eq!(name_problem(good), None, "{good}");
        }
        for bad in [
            "ab",
            "",
            "has space",
            "tëst",
            "seventeen_chars_x",
            "dash-ed",
        ] {
            assert!(name_problem(bad).is_some(), "{bad}");
        }
        assert_eq!(
            name_problem("tëst"),
            Some("Only letters, numbers and underscores.")
        );
    }
}
