//! Settings: the look, the game defaults, and the Microsoft client id.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use mctui_core::config::{Config, DEFAULT_JVM_ARGS, DEFAULT_MSA_CLIENT_ID};
use ratatui::layout::Rect;

use crate::app::{Action, Banner, Question};
use crate::data::Data;
use crate::overlay::HelpGroup;
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::theme::Palette;
use crate::ui::form::{Control, Field, Form, FormEvent};
use crate::ui::{Canvas, Hint, Target};

/// The widest the form grows.
const WIDTH: u16 = 76;

#[derive(Debug)]
pub struct Settings {
    form: Form,
    body: Rect,
    /// The theme when the screen opened, restored if changes are discarded.
    original_theme: String,
}

impl Settings {
    pub fn new(data: &Data) -> Self {
        let config = &data.config;
        let themes = std::iter::once("auto")
            .chain(Palette::ALL.iter().map(|p| p.name))
            .map(str::to_owned)
            .collect();
        let java = config
            .java_path
            .as_deref()
            .map(Path::display)
            .map(|p| p.to_string())
            .unwrap_or_default();
        let client_id = if config.msa_client_id == DEFAULT_MSA_CLIENT_ID {
            ""
        } else {
            &config.msa_client_id
        };
        let form = Form::new(vec![
            Field::new("theme", "Theme", Control::choice(themes, &config.theme))
                .section("Appearance")
                .help("←→ previews each theme. auto follows your terminal's light or dark background."),
            Field::new("snapshots", "Show snapshots", Control::Toggle(config.show_snapshots))
                .section("Game")
                .help("List snapshot versions when creating an instance."),
            Field::new("java", "Java", Control::text(&java, "Automatic"))
                .help("Leave empty and mctui finds or downloads the right Java for each version."),
            Field::new("jvm_args", "JVM arguments", Control::text(&config.jvm_args.join(" "), "-Xmx2G -Xms512M"))
                .help("Used by every instance that doesn't set its own."),
            Field::new("client_id", "Microsoft client ID", Control::text(client_id, "mctui's own"))
                .section("Account")
                .help("Only change this if you registered your own Azure app for sign-in."),
        ]);
        Self {
            form,
            body: Rect::default(),
            original_theme: config.theme.clone(),
        }
    }

    fn on_form(&mut self, event: FormEvent, cx: &mut Ctx<'_>) -> Handled {
        match event {
            FormEvent::Ignored => Handled::Ignored,
            // Themes preview as you cycle through them.
            FormEvent::Edited(i) if self.form.id(i) == "theme" => {
                Handled::Action(Action::SetTheme(self.form.choice("theme").to_owned()))
            }
            FormEvent::Moved | FormEvent::Edited(_) => Handled::Consumed,
            FormEvent::Submit => self.save(cx),
            FormEvent::Cancel if self.form.is_dirty() => {
                Handled::Action(Action::Confirm(Box::new(Question {
                    title: "Discard changes?".into(),
                    message: "Your changes to settings haven't been saved.".into(),
                    yes: "Discard",
                    danger: None,
                    on_yes: Action::Many(vec![
                        Action::SetTheme(self.original_theme.clone()),
                        Action::Close,
                    ]),
                })))
            }
            FormEvent::Cancel => Handled::Action(Action::Close),
        }
    }

    fn save(&mut self, cx: &mut Ctx<'_>) -> Handled {
        let java = self.form.text("java").trim();
        if !java.is_empty() && !Path::new(java).is_file() {
            self.form
                .set_error("java", Some("There's no Java at that path.".into()));
            return Handled::Consumed;
        }
        let jvm_args: Vec<String> = self
            .form
            .text("jvm_args")
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let client_id = self.form.text("client_id").trim();
        let config = Config {
            theme: self.form.choice("theme").to_owned(),
            show_snapshots: self.form.toggle("snapshots"),
            java_path: (!java.is_empty()).then(|| PathBuf::from(java)),
            jvm_args: if jvm_args.is_empty() {
                DEFAULT_JVM_ARGS.map(str::to_owned).to_vec()
            } else {
                jvm_args
            },
            msa_client_id: if client_id.is_empty() {
                DEFAULT_MSA_CLIENT_ID.to_owned()
            } else {
                client_id.to_owned()
            },
            ..cx.data.config.clone()
        };
        match config.save(&cx.data.dir) {
            Ok(()) => {
                cx.data.config = config;
                Handled::Action(Action::Many(vec![
                    Action::Banner(Banner::success("Settings saved.")),
                    Action::Close,
                ]))
            }
            Err(e) => Handled::Action(Action::Banner(Banner::warn(format!(
                "Couldn't save settings: {e}"
            )))),
        }
    }
}

impl Screen for Settings {
    fn crumbs(&self, _data: &Data) -> Vec<String> {
        vec!["Settings".into()]
    }

    fn hints(&self, _data: &Data) -> Vec<Hint> {
        let mut hints = vec![Hint::key(KeyCode::Enter, "save")];
        match &self.form.field(self.form.id(self.form.focus())).control {
            Control::Choice { .. } => hints.push(Hint::show("←→", "change")),
            Control::Toggle(_) => hints.push(Hint::key(KeyCode::Char(' '), "toggle")),
            Control::Text { .. } => {}
        }
        hints.push(Hint::key(KeyCode::Tab, "next"));
        hints.push(Hint::key(
            KeyCode::Esc,
            if self.form.is_dirty() {
                "discard"
            } else {
                "back"
            },
        ));
        hints
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        let groups = vec![
            HelpGroup {
                title: "Move",
                entries: &[("↑↓ tab", "next · previous field")],
            },
            HelpGroup {
                title: "Edit",
                entries: &[
                    ("←→", "change a choice"),
                    ("space", "toggle"),
                    ("type", "edit text"),
                ],
            },
            HelpGroup {
                title: "General",
                entries: &[
                    ("↵", "save"),
                    ("esc", "back · discard changes"),
                    ("?", "help"),
                ],
            },
        ];
        ("Settings", groups)
    }

    fn captures_text(&self) -> bool {
        self.form.captures_text()
    }

    fn set_body(&mut self, body: Rect) {
        self.body = body;
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        let event = self.form.handle_key(key);
        self.on_form(event, cx)
    }

    fn handle_mouse(&mut self, target: &Target, mouse: Mouse, cx: &mut Ctx<'_>) -> Handled {
        match (target, mouse) {
            (Target::Field(i), Mouse::Click { .. }) => {
                let event = self.form.click(*i);
                self.on_form(event, cx)
            }
            _ => Handled::Ignored,
        }
    }

    fn handle_paste(&mut self, text: &str, cx: &mut Ctx<'_>) -> Handled {
        let event = self.form.handle_paste(text);
        self.on_form(event, cx)
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
    use crossterm::event::KeyCode;

    use crate::app::tests::Harness;

    #[test]
    fn saves_and_previews_themes() {
        let mut h = Harness::new(100, 30);
        h.press(KeyCode::Char('s'));
        insta::assert_snapshot!(h.text());
        // Cycle the theme: it previews at once.
        // auto (dark here) → dark → light.
        h.press(KeyCode::Right);
        h.press(KeyCode::Right);
        assert_eq!(h.app_theme(), "light");
        h.press(KeyCode::Down);
        h.press(KeyCode::Char(' '));
        h.press(KeyCode::Enter);
        assert!(h.data().config.show_snapshots);
        assert_eq!(h.banner(), Some("Settings saved."));
        let saved = mctui_core::config::Config::load(&h.data().dir).value;
        assert!(saved.show_snapshots, "written to disk");
    }

    #[test]
    fn discarding_restores_the_theme() {
        let mut h = Harness::new(100, 30);
        h.press(KeyCode::Char('s'));
        let before = h.app_theme();
        h.press(KeyCode::Right);
        h.press(KeyCode::Esc);
        assert!(h.text().contains("Discard changes?"));
        h.press(KeyCode::Char('y'));
        assert_eq!(h.app_theme(), before);
        assert!(h.text().contains("INSTANCES"), "back on home");
    }

    #[test]
    fn a_missing_java_path_blocks_saving() {
        let mut h = Harness::new(100, 30);
        h.press(KeyCode::Char('s'));
        h.press(KeyCode::Down);
        h.press(KeyCode::Down);
        h.type_text("/nope/java");
        h.press(KeyCode::Enter);
        assert!(h.text().contains("There's no Java at that path."));
        assert!(h.data().config.java_path.is_none());
    }
}
