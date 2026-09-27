//! The accounts screen: who you play as. ↵ switches, n signs in with
//! Microsoft, o adds an offline profile, d removes.

use crossterm::event::{KeyCode, KeyEvent};
use mctui_core::account::{Account, AccountKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;

use super::{Change, SessionStatus};
use crate::app::{Action, Banner, Question, Route};
use crate::data::Data;
use crate::overlay::{HelpGroup, MenuItem, PaletteItem};
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::ui::list::List;
use crate::ui::widgets::{self, ButtonKind, Tone};
use crate::ui::{Canvas, Hint, Target, text};

/// The "+ Add Microsoft account" row. Account ids are hex, so it can't clash.
const ADD: &str = "+";
/// The widest the list grows: wider, and names drift away from their state.
const WIDTH: u16 = 72;
/// How far a row's content sits inside its band (see `widgets::row`).
const ROW_INSET: u16 = 2;

#[derive(Debug)]
pub struct AccountList {
    list: List,
    /// The account ids the rows were built from, to spot new ones.
    ids: Vec<String>,
    body: Rect,
}

impl AccountList {
    pub fn new(data: &Data) -> Self {
        let mut screen = Self {
            list: List::new("accounts"),
            ids: Vec::new(),
            body: Rect::default(),
        };
        screen.sync(data);
        if let Some(id) = &data.accounts.active_id {
            screen.list.select_key(id);
        }
        screen
    }

    /// Rebuilds the rows from `data`, selecting an account that just appeared.
    fn sync(&mut self, data: &Data) {
        let ids: Vec<String> = data
            .accounts
            .accounts
            .iter()
            .map(|a| a.id.clone())
            .collect();
        let added = ids.iter().find(|id| !self.ids.contains(id)).cloned();
        let first = self.ids.is_empty() && self.list.is_empty();
        self.list.set_keys(rows(data));
        if let Some(id) = added.filter(|_| !first) {
            self.list.select_key(&id);
        }
        self.ids = ids;
    }

    fn selected<'d>(&self, data: &'d Data) -> Option<&'d Account> {
        let key = self.list.selected_key()?;
        data.accounts.accounts.iter().find(|a| a.id == key)
    }

    /// ↵ and double-click: play as the selected account, or sign in from the
    /// add row.
    fn activate(&self, data: &Data) -> Handled {
        Handled::Action(match self.selected(data) {
            Some(account) if is_active(data, account) => Action::Banner(Banner::info(format!(
                "You're already playing as {}.",
                account.name
            ))),
            Some(account) => Action::Account(Change::Activate(account.id.clone())),
            None => sign_in(),
        })
    }

    /// The row's actions, at the pointer.
    fn menu(&self, data: &Data, anchor: Position) -> Handled {
        let Some(account) = self.selected(data) else {
            return Handled::Consumed;
        };
        let mut items = Vec::new();
        if !is_active(data, account) {
            items.push(MenuItem {
                label: "Play as this account",
                key: KeyCode::Enter,
                danger: false,
                action: Action::Account(Change::Activate(account.id.clone())),
            });
        }
        items.push(MenuItem {
            label: "Remove",
            key: KeyCode::Char('d'),
            danger: true,
            action: remove(account),
        });
        Handled::Action(Action::Menu {
            title: account.name.clone(),
            items,
            anchor: Some(anchor),
        })
    }

    fn render_row(
        cv: &mut Canvas<'_>,
        area: Rect,
        account: Option<&Account>,
        data: &Data,
        (selected, hovered): (bool, bool),
    ) {
        let t = *cv.theme();
        let label = widgets::row_label(&t, selected, hovered);
        let Some(account) = account else {
            let style = if selected || hovered {
                label
            } else {
                Style::new().fg(t.muted)
            };
            cv.text(area.x, area.y, "+ Add Microsoft account", area.width, style);
            return;
        };
        let expired = data.sessions.get(&account.id) == Some(&SessionStatus::Expired);
        let (chip, tone) = match (is_active(data, account), expired) {
            (true, true) => ("▲ Active · sign-in expired", Tone::Warning),
            (false, true) => ("▲ Sign-in expired", Tone::Warning),
            (true, false) => ("● Active", Tone::Success),
            (false, false) => (kind_label(&account.kind), Tone::Neutral),
        };
        let fg = match tone {
            Tone::Warning => t.warning,
            Tone::Success => t.success,
            _ if selected => t.subtle,
            _ => t.dim,
        };
        let chip = Line::styled(chip, Style::new().fg(fg));
        let chip_w = chip.width() as u16 + 2;
        // Flush with the end of the row's band, so no sliver of it shows past the chip.
        let x = (area.right() + ROW_INSET)
            .saturating_sub(chip_w)
            .max(area.x);
        let chip = widgets::chip(cv, x, area.y, &chip, tone, None, chip_w);
        let room = chip.x.saturating_sub(area.x + 2);
        let name = text::truncate(&account.name, usize::from(room));
        cv.text(area.x, area.y, &name, room, label);
    }

    fn render_empty(cv: &mut Canvas<'_>, area: Rect) {
        let t = *cv.theme();
        widgets::section(cv, area, "Accounts", None, true);
        let (x, w) = (area.x, area.width);
        let lines = text::wrap(
            "Sign in with Microsoft to play online, or add an offline profile.",
            usize::from(w),
        );
        let mut y = area.y + 2;
        for line in &lines {
            cv.text(x, y, line, w, Style::new().fg(t.dim));
            y += 1;
        }
        let end = widgets::button(
            cv,
            x,
            y + 1,
            "Sign in with Microsoft",
            Some("↵"),
            ButtonKind::Primary,
            Target::Key(KeyCode::Enter),
        )
        .right();
        widgets::button(
            cv,
            end + 2,
            y + 1,
            "Offline profile",
            Some("o"),
            ButtonKind::Secondary,
            Target::Key(KeyCode::Char('o')),
        );
    }
}

/// The rows: every account, then "+ Add Microsoft account".
fn rows(data: &Data) -> Vec<String> {
    data.accounts
        .accounts
        .iter()
        .map(|a| a.id.clone())
        .chain([ADD.to_owned()])
        .collect()
}

fn is_active(data: &Data, account: &Account) -> bool {
    data.accounts.active_id.as_deref() == Some(account.id.as_str())
}

fn kind_label(kind: &AccountKind) -> &str {
    match kind {
        AccountKind::Msa => "Microsoft",
        AccountKind::Offline => "Offline",
        AccountKind::Other(kind) if kind.is_empty() => "Unknown",
        AccountKind::Other(kind) => kind,
    }
}

fn sign_in() -> Action {
    Action::Open(Route::SignIn { then: None })
}

/// Asks before removing `account`.
fn remove(account: &Account) -> Action {
    let (title, message, danger) = match account.kind {
        AccountKind::Offline => (
            "Remove offline profile?",
            format!("Remove the offline profile “{}”?", account.name),
            None,
        ),
        _ => (
            "Remove account?",
            format!("Remove “{}” from mctui?", account.name),
            Some("You'll have to sign in again to use it.".to_owned()),
        ),
    };
    Action::Confirm(Box::new(Question {
        title: title.into(),
        message,
        yes: "Remove",
        danger,
        on_yes: Action::Account(Change::Remove(account.id.clone())),
    }))
}

impl Screen for AccountList {
    fn crumbs(&self, _data: &Data) -> Vec<String> {
        vec!["Accounts".into()]
    }

    fn hints(&self, data: &Data) -> Vec<Hint> {
        let key = Hint::key;
        let Some(account) = self.selected(data) else {
            return vec![
                key(KeyCode::Enter, "sign in"),
                key(KeyCode::Char('o'), "offline profile"),
                key(KeyCode::Esc, "back"),
                key(KeyCode::Char('?'), "help"),
            ];
        };
        let mut hints = Vec::new();
        if !is_active(data, account) {
            hints.push(key(KeyCode::Enter, "switch"));
        }
        hints.extend([
            key(KeyCode::Char('n'), "add account"),
            key(KeyCode::Char('o'), "offline profile"),
            key(KeyCode::Char('d'), "remove"),
            key(KeyCode::Esc, "back"),
            key(KeyCode::Char('?'), "help"),
        ]);
        hints
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        let groups = vec![
            HelpGroup {
                title: "Move",
                entries: &[("↑↓ j k", "move"), ("pgup pgdn", "page")],
            },
            HelpGroup {
                title: "Act",
                entries: &[
                    ("↵", "switch account · sign in"),
                    ("n", "add a Microsoft account"),
                    ("o", "add an offline profile"),
                    ("d ⌫", "remove"),
                ],
            },
            HelpGroup {
                title: "General",
                entries: &[
                    (":", "command palette"),
                    ("esc", "back"),
                    ("?", "help"),
                    ("double-click", "switch account"),
                    ("right-click", "actions"),
                ],
            },
        ];
        ("Accounts", groups)
    }

    fn palette(&self, _data: &Data) -> Vec<PaletteItem> {
        let verb = |label: &str, key, action| PaletteItem {
            label: label.to_owned(),
            key: Some(key),
            jump: false,
            action,
        };
        vec![
            verb("Add Microsoft account", "n", sign_in()),
            verb(
                "Add offline profile",
                "o",
                Action::Open(Route::OfflineProfile),
            ),
        ]
    }

    fn set_body(&mut self, body: Rect) {
        self.body = body;
        // Below the section label and its blank line.
        self.list.set_height(body.height.saturating_sub(2));
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        if let Some(moved) = self.list.handle_key(key) {
            return Handled::changed(moved);
        }
        let selected = self.selected(cx.data);
        match key.code {
            KeyCode::Enter => self.activate(cx.data),
            KeyCode::Char('n') => Handled::Action(sign_in()),
            KeyCode::Char('o') => Handled::Action(Action::Open(Route::OfflineProfile)),
            KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                selected.map_or(Handled::Ignored, |account| Handled::Action(remove(account)))
            }
            _ => Handled::Ignored,
        }
    }

    fn handle_mouse(&mut self, target: &Target, mouse: Mouse, cx: &mut Ctx<'_>) -> Handled {
        let Some(key) = self.list.key_of(target).map(str::to_owned) else {
            return Handled::Ignored;
        };
        match mouse {
            Mouse::Scroll { down } => Handled::changed(self.list.step(if down { 1 } else { -1 })),
            Mouse::Click { double } => {
                self.list.select_key(&key);
                if double {
                    self.activate(cx.data)
                } else {
                    Handled::Consumed
                }
            }
            Mouse::RightClick(at) => {
                self.list.select_key(&key);
                self.menu(cx.data, at)
            }
        }
    }

    fn reload(&mut self, data: &Data) {
        self.sync(data);
    }

    fn render(&self, cv: &mut Canvas<'_>, data: &Data) {
        let area = Rect {
            width: self.body.width.min(WIDTH),
            ..self.body
        };
        if data.accounts.accounts.is_empty() {
            let inset = Rect {
                x: area.x + 2,
                width: area.width.saturating_sub(4),
                ..area
            };
            Self::render_empty(cv, inset);
            return;
        }
        let title = Rect::new(area.x + 2, area.y, area.width.saturating_sub(4), 1);
        let count = data.accounts.accounts.len().to_string();
        widgets::section(cv, title, "Accounts", Some(&count), true);
        let rows_area = Rect {
            y: area.y + 2,
            height: area.height.saturating_sub(2),
            ..area
        };
        let accounts = &data.accounts.accounts;
        self.list
            .render(cv, rows_area, true, |cv, inner, i, selected, hovered| {
                Self::render_row(cv, inner, accounts.get(i), data, (selected, hovered));
            });
    }
}
