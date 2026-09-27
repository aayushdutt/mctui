//! Forms: labelled fields you move between, edit in place, and save with ↵.
//!
//! ↑↓ / tab move between fields, ↵ submits from anywhere, esc cancels. Text
//! fields take typing; toggles flip with space or ←→; choices cycle with ←→.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::input::TextInput;
use super::{Canvas, Target, text, widgets};

/// Width of the label column.
pub const LABEL_W: u16 = 22;
/// Widest a text field grows.
const FIELD_MAX: u16 = 52;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Text {
        input: TextInput,
        placeholder: &'static str,
    },
    Toggle(bool),
    Choice {
        options: Vec<String>,
        index: usize,
    },
}

impl Control {
    pub fn text(value: &str, placeholder: &'static str) -> Self {
        Self::Text {
            input: TextInput::new(value),
            placeholder,
        }
    }

    /// A choice starting at `value` (or the first option if it isn't one).
    pub fn choice(options: Vec<String>, value: &str) -> Self {
        let index = options.iter().position(|o| o == value).unwrap_or(0);
        Self::Choice { options, index }
    }

    /// The value as text, for dirty checks.
    fn value(&self) -> String {
        match self {
            Control::Text { input, .. } => input.as_str().to_owned(),
            Control::Toggle(on) => on.to_string(),
            Control::Choice { options, index } => options.get(*index).cloned().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Stable id for lookups.
    pub id: &'static str,
    pub label: &'static str,
    /// Shown under the form while the field has focus.
    pub help: &'static str,
    /// A section label drawn above this field, starting a group.
    pub section: Option<&'static str>,
    pub control: Control,
    pub error: Option<String>,
}

impl Field {
    pub fn new(id: &'static str, label: &'static str, control: Control) -> Self {
        Self {
            id,
            label,
            help: "",
            section: None,
            control,
            error: None,
        }
    }

    pub fn help(mut self, help: &'static str) -> Self {
        self.help = help;
        self
    }

    pub fn section(mut self, label: &'static str) -> Self {
        self.section = Some(label);
        self
    }
}

/// What a key or click did to the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormEvent {
    Ignored,
    /// Focus moved, or something else changed that needs a redraw.
    Moved,
    /// The value of field `i` changed.
    Edited(usize),
    Submit,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct Form {
    fields: Vec<Field>,
    focus: usize,
    initial: Vec<String>,
}

impl Form {
    pub fn new(fields: Vec<Field>) -> Self {
        let initial = fields.iter().map(|f| f.control.value()).collect();
        Self {
            fields,
            focus: 0,
            initial,
        }
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    fn index(&self, id: &str) -> usize {
        self.fields
            .iter()
            .position(|f| f.id == id)
            .unwrap_or_else(|| panic!("no form field {id:?}"))
    }

    pub fn field(&self, id: &str) -> &Field {
        &self.fields[self.index(id)]
    }

    /// A text field's value.
    pub fn text(&self, id: &str) -> &str {
        match &self.field(id).control {
            Control::Text { input, .. } => input.as_str(),
            _ => "",
        }
    }

    pub fn toggle(&self, id: &str) -> bool {
        matches!(self.field(id).control, Control::Toggle(true))
    }

    /// A choice field's current option.
    pub fn choice(&self, id: &str) -> &str {
        match &self.field(id).control {
            Control::Choice { options, index } => options.get(*index).map_or("", String::as_str),
            _ => "",
        }
    }

    pub fn id(&self, i: usize) -> &'static str {
        self.fields[i].id
    }

    /// Shows `error` under a field (or clears it) and focuses the first field
    /// with an error.
    pub fn set_error(&mut self, id: &str, error: Option<String>) {
        let i = self.index(id);
        self.fields[i].error = error;
        if let Some(first) = self.fields.iter().position(|f| f.error.is_some()) {
            self.focus = first;
        }
    }

    /// Whether anything differs from how the form started.
    pub fn is_dirty(&self) -> bool {
        self.fields
            .iter()
            .zip(&self.initial)
            .any(|(f, v)| f.control.value() != *v)
    }

    /// Whether keys go into a text field.
    pub fn captures_text(&self) -> bool {
        matches!(self.fields[self.focus].control, Control::Text { .. })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> FormEvent {
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let typing = self.captures_text();
        match key.code {
            KeyCode::Enter => return FormEvent::Submit,
            KeyCode::Esc => return FormEvent::Cancel,
            KeyCode::Tab => return self.move_focus(1, true),
            KeyCode::BackTab => return self.move_focus(-1, true),
            KeyCode::Down => return self.move_focus(1, false),
            KeyCode::Up => return self.move_focus(-1, false),
            KeyCode::Char('j') if plain && !typing => return self.move_focus(1, false),
            KeyCode::Char('k') if plain && !typing => return self.move_focus(-1, false),
            _ => {}
        }
        let i = self.focus;
        let changed = match &mut self.fields[i].control {
            Control::Text { input, .. } => input.handle_key(key).unwrap_or(false),
            Control::Toggle(on) => match key.code {
                KeyCode::Char(' ' | 'h' | 'l') | KeyCode::Left | KeyCode::Right => {
                    *on = !*on;
                    true
                }
                _ => false,
            },
            Control::Choice { options, index } => match key.code {
                KeyCode::Left | KeyCode::Char('h') => cycle(index, options.len(), false),
                KeyCode::Right | KeyCode::Char('l' | ' ') => cycle(index, options.len(), true),
                _ => false,
            },
        };
        self.edited(i, changed)
    }

    pub fn handle_paste(&mut self, pasted: &str) -> FormEvent {
        let i = self.focus;
        let changed = match &mut self.fields[i].control {
            Control::Text { input, .. } => input.insert(pasted),
            _ => false,
        };
        self.edited(i, changed)
    }

    /// A click on field `i`: focuses it, and flips a toggle or advances a choice.
    pub fn click(&mut self, i: usize) -> FormEvent {
        if i >= self.fields.len() {
            return FormEvent::Ignored;
        }
        let was_focused = self.focus == i;
        self.focus = i;
        let changed = match &mut self.fields[i].control {
            Control::Toggle(on) => {
                *on = !*on;
                true
            }
            Control::Choice { options, index } if was_focused => cycle(index, options.len(), true),
            _ => false,
        };
        if changed {
            self.edited(i, true)
        } else {
            FormEvent::Moved
        }
    }

    fn edited(&mut self, i: usize, changed: bool) -> FormEvent {
        if !changed {
            return FormEvent::Ignored;
        }
        self.fields[i].error = None;
        FormEvent::Edited(i)
    }

    fn move_focus(&mut self, delta: isize, wrap: bool) -> FormEvent {
        let n = self.fields.len();
        let next = if wrap {
            self.focus
                .checked_add_signed(delta)
                .map_or(n - 1, |i| i % n)
        } else {
            self.focus.saturating_add_signed(delta).min(n - 1)
        };
        if std::mem::replace(&mut self.focus, next) == next {
            FormEvent::Ignored
        } else {
            FormEvent::Moved
        }
    }

    /// Draws the fields in `area`: labels in a column, controls beside them,
    /// errors in the gap below, the focused field's help at the end.
    pub fn render(
        &self,
        cv: &mut Canvas<'_>,
        area: Rect,
        focused: bool,
        target: impl Fn(usize) -> Target,
    ) {
        let t = *cv.theme();
        let mut y = area.y;
        for (i, field) in self.fields.iter().enumerate() {
            if let Some(label) = field.section {
                if i > 0 {
                    y += 1;
                }
                widgets::section(cv, Rect::new(area.x, y, area.width, 1), label, None, false);
                y += 2;
            }
            if y >= area.bottom() {
                break;
            }
            let is_focus = focused && i == self.focus;
            let label = if is_focus {
                Style::new().fg(t.strong).bold()
            } else {
                Style::new().fg(t.dim)
            };
            cv.text(area.x, y, field.label, LABEL_W, label);
            let x = area.x + LABEL_W;
            let w = area.right().saturating_sub(x).min(FIELD_MAX);
            match &field.control {
                Control::Text { input, placeholder } => {
                    widgets::field(
                        cv,
                        Rect::new(x, y, w, 1),
                        input,
                        placeholder,
                        is_focus,
                        Some(target(i)),
                    );
                }
                Control::Toggle(on) => {
                    widgets::toggle(cv, x, y, *on, is_focus, target(i));
                }
                Control::Choice { options, index } => {
                    let value = options.get(*index).map_or("", String::as_str);
                    widgets::choice(cv, x, y, value, is_focus, target(i));
                }
            }
            if let Some(error) = &field.error {
                let msg = format!("{} {error}", super::CROSS);
                cv.text(
                    x + 1,
                    y + 1,
                    &text::truncate(&msg, usize::from(w)),
                    w,
                    Style::new().fg(t.error),
                );
            }
            y += 2;
        }
        let help = self.fields.get(self.focus).map_or("", |f| f.help);
        if focused && !help.is_empty() {
            let x = area.x + LABEL_W + 1;
            let w = area.right().saturating_sub(x);
            for (i, line) in text::wrap(help, usize::from(w)).iter().take(2).enumerate() {
                let y = y + i as u16;
                if y < area.bottom() {
                    cv.text(x, y, line, w, Style::new().fg(t.muted));
                }
            }
        }
    }
}

fn cycle(index: &mut usize, len: usize, forward: bool) -> bool {
    if len < 2 {
        return false;
    }
    *index = if forward {
        (*index + 1) % len
    } else {
        (*index + len - 1) % len
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form::new(vec![
            Field::new("name", "Name", Control::text("Survival", "")),
            Field::new("snapshots", "Show snapshots", Control::Toggle(false)),
            Field::new(
                "theme",
                "Theme",
                Control::choice(vec!["auto".into(), "dark".into()], "dark"),
            ),
        ])
    }

    fn press(form: &mut Form, code: KeyCode) -> FormEvent {
        form.handle_key(KeyEvent::from(code))
    }

    #[test]
    fn typing_edits_and_marks_dirty() {
        let mut f = form();
        assert!(f.captures_text());
        assert_eq!(press(&mut f, KeyCode::Char('!')), FormEvent::Edited(0));
        assert_eq!(f.text("name"), "Survival!");
        assert!(f.is_dirty());
        press(&mut f, KeyCode::Backspace);
        assert!(!f.is_dirty(), "back to how it started");
    }

    #[test]
    fn arrows_stop_at_the_ends_and_tab_wraps() {
        let mut f = form();
        assert_eq!(press(&mut f, KeyCode::Up), FormEvent::Ignored);
        press(&mut f, KeyCode::Down);
        press(&mut f, KeyCode::Down);
        assert_eq!(press(&mut f, KeyCode::Down), FormEvent::Ignored);
        assert_eq!(press(&mut f, KeyCode::Tab), FormEvent::Moved);
        assert_eq!(f.focus(), 0);
        press(&mut f, KeyCode::BackTab);
        assert_eq!(f.focus(), 2);
    }

    #[test]
    fn toggles_and_choices() {
        let mut f = form();
        press(&mut f, KeyCode::Down);
        press(&mut f, KeyCode::Char(' '));
        assert!(f.toggle("snapshots"));
        press(&mut f, KeyCode::Down);
        press(&mut f, KeyCode::Right);
        assert_eq!(f.choice("theme"), "auto");
        press(&mut f, KeyCode::Left);
        assert_eq!(f.choice("theme"), "dark");
        // j/k move when not typing.
        press(&mut f, KeyCode::Char('k'));
        assert_eq!(f.focus(), 1);
    }

    #[test]
    fn enter_submits_anywhere_and_errors_focus_their_field() {
        let mut f = form();
        press(&mut f, KeyCode::Down);
        assert_eq!(press(&mut f, KeyCode::Enter), FormEvent::Submit);
        f.set_error("name", Some("taken".into()));
        assert_eq!(f.focus(), 0);
        assert!(f.field("name").error.is_some());
        press(&mut f, KeyCode::Char('x'));
        assert!(f.field("name").error.is_none(), "editing clears the error");
    }

    #[test]
    fn clicks_focus_and_flip() {
        let mut f = form();
        assert_eq!(f.click(1), FormEvent::Edited(1));
        assert!(f.toggle("snapshots"));
        assert_eq!(
            f.click(2),
            FormEvent::Moved,
            "the first click only focuses a choice"
        );
        assert_eq!(f.click(2), FormEvent::Edited(2));
    }
}
