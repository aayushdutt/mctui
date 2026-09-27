//! Edit instance: its name, memory, Java and JVM arguments.
//!
//! Memory is stored as a leading `-Xmx` in the instance's JVM arguments, so
//! older builds read it too; at launch the instance's arguments override the
//! global ones flag by flag rather than replacing them all.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use mctui_core::instance::Instance;
use ratatui::layout::Rect;

use crate::app::{Action, Banner, Question};
use crate::data::Data;
use crate::overlay::HelpGroup;
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::ui::form::{Control, Field, Form, FormEvent};
use crate::ui::{Canvas, Hint, Target};

const WIDTH: u16 = 76;

#[derive(Debug)]
pub struct EditInstance {
    id: String,
    name: String,
    form: Form,
    body: Rect,
}

impl EditInstance {
    pub fn new(inst: &Instance) -> Self {
        let (memory, args) = split_memory(&inst.jvm_args);
        let java = inst
            .java_path
            .as_deref()
            .map(Path::display)
            .map(|p| p.to_string())
            .unwrap_or_default();
        let form = Form::new(vec![
            Field::new("name", "Name", Control::text(&inst.name, "Instance name")).help("Only the name changes; the folder stays put."),
            Field::new("memory", "Memory", Control::text(memory.unwrap_or_default(), "Default"))
                .section("Performance")
                .help("How much memory the game may use, e.g. 4G or 6144M. Empty uses the default from settings."),
            Field::new("java", "Java", Control::text(&java, "Automatic"))
                .help("Leave empty and mctui picks the right Java for this version."),
            Field::new("jvm_args", "JVM arguments", Control::text(&args.join(" "), "None"))
                .help("Extra arguments for this instance, added to the ones in settings."),
        ]);
        Self {
            id: inst.id.clone(),
            name: inst.name.clone(),
            form,
            body: Rect::default(),
        }
    }

    fn on_form(&mut self, event: FormEvent, cx: &mut Ctx<'_>) -> Handled {
        match event {
            FormEvent::Ignored => Handled::Ignored,
            FormEvent::Moved | FormEvent::Edited(_) => Handled::Consumed,
            FormEvent::Submit => self.save(cx),
            FormEvent::Cancel if self.form.is_dirty() => {
                Handled::Action(Action::Confirm(Box::new(Question {
                    title: "Discard changes?".into(),
                    message: format!("Your changes to “{}” haven't been saved.", self.name),
                    yes: "Discard",
                    danger: None,
                    on_yes: Action::Close,
                })))
            }
            FormEvent::Cancel => Handled::Action(Action::Close),
        }
    }

    fn save(&mut self, cx: &mut Ctx<'_>) -> Handled {
        let name = self.form.text("name").trim().to_owned();
        let memory = self.form.text("memory").trim().to_owned();
        let xmx = memory_arg(&memory);
        let java = self.form.text("java").trim().to_owned();
        if name.is_empty() {
            self.form.set_error("name", Some("Give it a name.".into()));
        }
        if !memory.is_empty() && xmx.is_none() {
            self.form.set_error(
                "memory",
                Some("Use a size like 4G or 6144M, at least 512M.".into()),
            );
        }
        if !java.is_empty() && !Path::new(&java).is_file() {
            self.form
                .set_error("java", Some("There's no Java at that path.".into()));
        }
        if ["name", "memory", "java"]
            .iter()
            .any(|id| self.form.field(id).error.is_some())
        {
            return Handled::Consumed;
        }

        let Some(inst) = cx.data.instances.iter_mut().find(|i| i.id == self.id) else {
            return Handled::Action(Action::Many(vec![
                Action::Banner(Banner::warn("That instance is gone.")),
                Action::Close,
            ]));
        };
        let mut edited = inst.clone();
        edited.name = name;
        edited.java_path = (!java.is_empty()).then(|| PathBuf::from(java));
        let extra = self
            .form
            .text("jvm_args")
            .split_whitespace()
            .map(str::to_owned);
        edited.jvm_args = xmx
            .map(|size| format!("-Xmx{size}"))
            .into_iter()
            .chain(extra)
            .collect();
        match cx.data.store.save(&edited) {
            Ok(()) => {
                let banner = Banner::success(format!("Saved “{}”.", edited.name));
                *inst = edited;
                Handled::Action(Action::Many(vec![
                    Action::Reload,
                    Action::Banner(banner),
                    Action::Close,
                ]))
            }
            Err(e) => Handled::Action(Action::Banner(Banner::warn(format!(
                "Couldn't save “{}”: {e}",
                self.name
            )))),
        }
    }
}

/// Splits a leading-or-anywhere `-Xmx` out of `args`: `(memory, the rest)`.
fn split_memory(args: &[String]) -> (Option<&str>, Vec<&str>) {
    let mut memory = None;
    let rest = args
        .iter()
        .filter_map(|a| match a.strip_prefix("-Xmx") {
            Some(m) if memory.is_none() => {
                memory = Some(m);
                None
            }
            _ => Some(a.as_str()),
        })
        .collect();
    (memory, rest)
}

/// The `-Xmx` size for what was typed: `4G`, `6144M`, or bare megabytes
/// (`4096`; Java would read those as bytes). `None` if it isn't a size, or is
/// too small for the game to start.
fn memory_arg(s: &str) -> Option<String> {
    let digits = s.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    let (unit, mb) = match &s[digits.len()..] {
        "" | "m" | "M" => ('M', n),
        "g" | "G" => ('G', n.saturating_mul(1024)),
        "k" | "K" => ('K', n / 1024),
        _ => return None,
    };
    (mb >= 512).then(|| format!("{n}{unit}"))
}

impl Screen for EditInstance {
    fn crumbs(&self, _data: &Data) -> Vec<String> {
        vec![self.name.clone(), "Edit".into()]
    }

    fn hints(&self, _data: &Data) -> Vec<Hint> {
        vec![
            Hint::key(KeyCode::Enter, "save"),
            Hint::key(KeyCode::Tab, "next"),
            Hint::key(
                KeyCode::Esc,
                if self.form.is_dirty() {
                    "discard"
                } else {
                    "back"
                },
            ),
        ]
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        let groups = vec![
            HelpGroup {
                title: "Move",
                entries: &[("↑↓ tab", "next · previous field")],
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
        ("Edit instance", groups)
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

    use super::*;
    use crate::app::tests::Harness;

    #[test]
    fn memory_is_validated_and_split() {
        assert_eq!(memory_arg("4g").as_deref(), Some("4G"));
        assert_eq!(memory_arg("6144m").as_deref(), Some("6144M"));
        assert_eq!(memory_arg("4096").as_deref(), Some("4096M"), "megabytes");
        for bad in ["4GB", "G", "four", "+4G", "256M", "4 G", ""] {
            assert_eq!(memory_arg(bad), None, "{bad}");
        }
        let args = vec![
            "-Xms512M".to_owned(),
            "-Xmx4G".to_owned(),
            "-Dfoo".to_owned(),
        ];
        assert_eq!(split_memory(&args), (Some("4G"), vec!["-Xms512M", "-Dfoo"]));
    }

    #[test]
    fn saves_name_memory_and_args() {
        let mut h = Harness::new(100, 30);
        h.press(KeyCode::Char('e'));
        insta::assert_snapshot!(h.text());
        h.type_text(" II");
        h.press(KeyCode::Tab);
        h.type_text("6g");
        h.press(KeyCode::Enter);
        assert_eq!(h.banner(), Some("Saved “Survival 1.21 II”."));
        let inst = h
            .data()
            .instances
            .iter()
            .find(|i| i.id == "Survival 1.21")
            .cloned()
            .unwrap_or_default();
        assert_eq!(inst.name, "Survival 1.21 II");
        assert_eq!(inst.jvm_args, ["-Xmx6G"]);
        let on_disk = h.data().store.scan().unwrap().instances;
        assert!(
            on_disk.iter().any(|i| i.name == "Survival 1.21 II"),
            "written to disk"
        );
    }

    #[test]
    fn bad_values_stay_on_screen_with_errors() {
        let mut h = Harness::new(100, 30);
        h.press(KeyCode::Char('e'));
        h.press(KeyCode::Tab);
        h.type_text("lots");
        h.press(KeyCode::Enter);
        assert!(
            h.text()
                .contains("Use a size like 4G or 6144M, at least 512M.")
        );
        h.press(KeyCode::Esc);
        assert!(
            h.text().contains("Discard changes?"),
            "edits are never dropped silently"
        );
    }
}
