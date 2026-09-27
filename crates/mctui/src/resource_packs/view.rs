//! Drawing the resource packs screen.
//!
//! Each pane is a title row, a blank, and a list of one-line rows; the packs
//! pane ends with the selected pack's details and your pack with its build
//! button. Where panes share a place, their titles become tabs.

use mctui_core::vanilla_tweaks::{self, PackState, Unsupported};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::browser::{Browser, CartRow, PackRow, Search, Work};
use super::layout::{self, Mode};
use super::{Command, PANE, Pane, ResourcePacks, State, TAB, target};
use crate::theme::Theme;
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{self, Canvas, text};

/// The widest the empty-state cards grow.
const CARD_W: u16 = 62;

impl ResourcePacks {
    pub(super) fn draw(&self, cv: &mut Canvas<'_>) {
        let body = self.body;
        match &self.state {
            State::Unsupported(why) => unsupported(cv, body, &self.inst.version, *why),
            State::Loading { since, .. } => {
                loading(cv, body, widgets::spinner(*since, self.frame));
            }
            State::Failed(message) => failed(cv, body, message),
            State::Ready(b) => {
                let lay = layout::layout(body, b.focus, b.offline);
                if let Some(pane) = lay.categories {
                    cv.clipped(pane, |cv| categories(cv, pane, b, lay.mode));
                }
                if let Some(pane) = lay.packs {
                    cv.clipped(pane, |cv| packs(cv, pane, b, lay.mode));
                }
                if let Some(pane) = lay.cart {
                    let spin = b
                        .job
                        .as_ref()
                        .map(|job| widgets::spinner(job.since, self.frame));
                    cv.clipped(pane, |cv| cart(cv, pane, b, lay.mode, spin));
                }
                if let Some(row) = lay.notice {
                    let spin = b
                        .retry
                        .as_ref()
                        .map(|(since, _)| widgets::spinner(*since, self.frame));
                    offline(cv, row, spin);
                }
            }
        }
    }
}

/// Rows the category's warning takes above the packs (none while searching).
pub(super) fn warning_lines(b: &Browser, pane_w: u16) -> u16 {
    category_warning(b).map_or(0, |warning| {
        wrapped(&warning, inner_w(pane_w), 2).len() as u16
    })
}

/// Rows your pack's footer takes: a gap, a note when the game isn't using
/// the pack, and the build button or the running job.
pub(super) fn cart_footer_height(b: &Browser, pane_w: u16) -> u16 {
    let note = off_note(b).map_or(0, |note| wrapped(note, inner_w(pane_w), 2).len() as u16 + 1);
    let action = u16::from(b.job.is_some() || b.needs_build());
    if note + action == 0 {
        0
    } else {
        1 + note + action
    }
}

/// Text inside a pane: inset 2 columns each side.
fn inner_w(pane_w: u16) -> usize {
    usize::from(pane_w.saturating_sub(4))
}

/// `s` wrapped to `width`, at most `max` lines, the last cut with "…" when
/// there was more.
fn wrapped(s: &str, width: usize, max: usize) -> Vec<String> {
    let mut lines = text::wrap(s, width);
    if lines.len() > max {
        lines.truncate(max);
        if let Some(last) = lines.last_mut() {
            let cut = text::truncate(last, width.saturating_sub(1)).into_owned();
            *last = format!("{}…", cut.trim_end_matches('…').trim_end());
        }
    }
    lines
}

fn category_warning(b: &Browser) -> Option<String> {
    if b.search.is_some() {
        return None;
    }
    let warning = b.selected_category()?.warning.as_ref()?;
    Some(format!(
        "{} {}",
        ui::WARN,
        vanilla_tweaks::plain_text(&warning.text)
    ))
}

/// Why the game isn't showing the built pack, when it isn't.
fn off_note(b: &Browser) -> Option<&'static str> {
    (b.is_built() && !b.enabled && b.job.is_none())
        .then_some("Minecraft isn't using your pack — press b to set it up again.")
}

fn loading(cv: &mut Canvas<'_>, body: Rect, spin: &str) {
    let t = *cv.theme();
    let label = "Loading packs from Vanilla Tweaks…";
    let w = text::width(label) as u16 + 3;
    let x = body.x + body.width.saturating_sub(w) / 2;
    let y = body.y + body.height.saturating_sub(2) / 2;
    cv.text(x, y, spin, 1, Style::new().fg(t.accent));
    cv.text(x + 3, y, label, w, Style::new().fg(t.subtle));
}

/// A centred card for when there's nothing to browse: a title, a few lines
/// of copy, and maybe one button.
fn empty_card(
    cv: &mut Canvas<'_>,
    body: Rect,
    title: &str,
    lines: &[(&str, Style)],
    button: Option<(&str, &str, Command)>,
) {
    let t = *cv.theme();
    let width = ui::card_width(body.width, CARD_W);
    let inner = usize::from(width.saturating_sub(4));
    let paragraphs: Vec<(Vec<String>, Style)> = lines
        .iter()
        .map(|(s, style)| (text::wrap(s, inner), *style))
        .collect();
    let copy_h: usize = paragraphs.iter().map(|(p, _)| p.len() + 1).sum();
    let body_h = copy_h.saturating_sub(1) + if button.is_some() { 2 } else { 0 };
    let rect = ui::card_rect(body, width, body_h as u16, None);
    let area = cv.card(rect, title, t.strong);
    let mut y = area.y;
    for (paragraph, style) in &paragraphs {
        for line in paragraph {
            cv.text(area.x, y, line, area.width, *style);
            y += 1;
        }
        y += 1;
    }
    if let Some((label, key, command)) = button {
        widgets::button(
            cv,
            area.x,
            y,
            label,
            Some(key),
            ButtonKind::Primary,
            command.button(),
        );
    }
}

fn unsupported(cv: &mut Canvas<'_>, body: Rect, version: &str, why: Unsupported) {
    let t = *cv.theme();
    let reason = why.to_string();
    empty_card(
        cv,
        body,
        &format!("No packs for Minecraft {version}"),
        &[
            (&reason, Style::new().fg(t.text)),
            (
                "Resource packs work on instances running a release, 1.8 or newer.",
                Style::new().fg(t.dim),
            ),
        ],
        None,
    );
}

fn failed(cv: &mut Canvas<'_>, body: Rect, message: &str) {
    let t = *cv.theme();
    empty_card(
        cv,
        body,
        "Couldn't load the packs",
        &[(message, Style::new().fg(t.text))],
        Some(("Try again", "r", Command::Retry)),
    );
}

fn offline(cv: &mut Canvas<'_>, row: Rect, spin: Option<&str>) {
    let t = *cv.theme();
    let line = match spin {
        Some(spin) => Line::from(vec![
            Span::styled(format!("{spin}  "), Style::new().fg(t.accent)),
            Span::styled("Trying Vanilla Tweaks again…", Style::new().fg(t.subtle)),
        ]),
        None => Line::styled(
            format!(
                "{}  Couldn't reach Vanilla Tweaks — these are the packs from your last visit.",
                ui::WARN
            ),
            Style::new().fg(t.warning),
        ),
    };
    cv.line(
        row.x,
        row.y,
        &text::fit(line, usize::from(row.width)),
        row.width,
    );
}

/// The pane's title row, inset like its rows' text.
fn title_row(pane: Rect) -> Rect {
    Rect::new(pane.x + 2, pane.y, pane.width.saturating_sub(4), 1)
}

/// The title row where panes share a place: a tab for each ([Categories,]
/// the packs, your pack) with `active` lit, and your pack's state on the
/// right.
fn tab_row(cv: &mut Canvas<'_>, pane: Rect, b: &Browser, mode: Mode, active: Pane) {
    let end = tabs(cv, pane, b, mode, active);
    cart_status(cv, title_row(pane), end, b);
}

/// The tabs of [`tab_row`]. Returns the x after the last one.
fn tabs(cv: &mut Canvas<'_>, pane: Rect, b: &Browser, mode: Mode, active: Pane) -> u16 {
    let packs_label = if b.search.is_some() {
        "Search"
    } else {
        b.selected_node().map_or("Packs", |node| node.name())
    };
    let packs_count = if b.search.is_some() {
        b.result_rows.len()
    } else {
        b.pack_rows.len()
    };
    let mut panes = Vec::with_capacity(3);
    let mut labels = Vec::with_capacity(3);
    if mode == Mode::Single {
        panes.push(Pane::Categories);
        labels.push(("Categories", None));
    }
    panes.extend([Pane::Packs, Pane::Cart]);
    labels.push((packs_label, Some(packs_count)));
    labels.push(("Your pack", Some(b.cart.len())));
    let index = panes.iter().position(|p| *p == active).unwrap_or(0);
    // Chips pad their text a cell each side: start a cell early so the text
    // lines up with the rows'.
    let row = Rect::new(pane.x + 1, pane.y, pane.width.saturating_sub(2), 1);
    widgets::tabs(cv, row, &labels, index, b.focus == active, |i| {
        target(TAB, panes.get(i).map_or("packs", |p| p.key()))
    });
    // Each chip is its text padded a cell each side, a cell apart.
    let chips: usize = labels
        .iter()
        .map(|(label, count)| text::width(label) + count.map_or(0, |n| n.to_string().len() + 1) + 3)
        .sum();
    (row.x + chips as u16).min(row.right())
}

fn categories(cv: &mut Canvas<'_>, pane: Rect, b: &Browser, mode: Mode) {
    let t = *cv.theme();
    cv.click(pane, target(PANE, Pane::Categories.key()));
    let focused = b.focus == Pane::Categories;
    if mode == Mode::Single {
        tab_row(cv, pane, b, mode, Pane::Categories);
    } else {
        widgets::section(cv, title_row(pane), "Categories", None, focused);
    }
    let list = layout::content(pane);
    let current = b.categories.selected();
    b.categories
        .render(cv, list, focused, |cv, inner, i, selected, hovered| {
            let Some(node) = b.nodes.get(i) else {
                return;
            };
            // Without the band, the category whose packs are showing keeps the
            // accent, so the tree still says where you are.
            let label = if i == current && !focused && !hovered {
                Style::new().fg(t.accent)
            } else {
                widgets::row_label(&t, selected, hovered)
            };
            let x = inner.x + node.depth * 2;
            if node.expandable {
                let glyph = if node.expanded { "▾" } else { "▸" };
                let fg = if selected || hovered { t.text } else { t.muted };
                cv.text(x, inner.y, glyph, 1, Style::new().fg(fg));
            }
            let right = if node.picked > 0 {
                let fg = if selected { t.strong } else { t.subtle };
                let count = Line::styled(node.picked.to_string(), Style::new().fg(fg));
                cv.line_right(inner, inner.y, &count).x
            } else {
                inner.right()
            };
            let room = right.saturating_sub(x + 2 + 1);
            let name = text::truncate(node.name(), usize::from(room));
            cv.text(x + 2, inner.y, &name, room, label);
        });
}

fn packs(cv: &mut Canvas<'_>, pane: Rect, b: &Browser, mode: Mode) {
    let t = *cv.theme();
    cv.click(pane, target(PANE, Pane::Packs.key()));
    let focused = b.focus == Pane::Packs;
    match (&b.search, mode) {
        (Some(search), _) => search_field(cv, pane, b, search),
        (None, Mode::Wide) => {
            let count = b.pack_rows.len().to_string();
            let title = b.selected_node().map_or("Packs", |node| node.key.as_str());
            widgets::section(cv, title_row(pane), title, Some(&count), focused);
        }
        (None, _) => tab_row(cv, pane, b, mode, Pane::Packs),
    }

    let area = layout::packs_area(pane, warning_lines(b, pane.width));
    if let (Some(rect), Some(warning)) = (area.warning, category_warning(b)) {
        let lines = wrapped(&warning, usize::from(rect.width.saturating_sub(4)), 2);
        for (i, line) in lines.iter().enumerate() {
            let y = rect.y + i as u16;
            cv.text(
                rect.x + 2,
                y,
                line,
                rect.width.saturating_sub(4),
                Style::new().fg(t.warning),
            );
        }
    }

    let (list, rows, searching) = match &b.search {
        Some(_) => (&b.results, &b.result_rows, true),
        None => (&b.packs, &b.pack_rows, false),
    };
    if rows.is_empty() {
        let empty = if searching {
            "No packs match."
        } else if b.selected_node().is_some_and(|node| node.expandable) {
            "Its packs are in the categories under it."
        } else {
            "No packs here."
        };
        cv.text(
            area.list.x + 2,
            area.list.y,
            empty,
            area.list.width,
            Style::new().fg(t.faint),
        );
    }
    list.render(cv, area.list, focused, |cv, inner, i, selected, hovered| {
        if let Some(row) = rows.get(i) {
            pack_row(cv, inner, row, (selected, hovered), searching);
        }
    });
    if let Some(details_area) = area.details {
        details(cv, details_area, b);
    }
}

/// While searching, the packs pane's title is the search field.
fn search_field(cv: &mut Canvas<'_>, pane: Rect, b: &Browser, search: &Search) {
    let t = *cv.theme();
    let row = Rect::new(pane.x, pane.y, pane.width, 1);
    let button = Command::Search.button();
    let bg = if search.typing {
        t.selection
    } else if cv.hovered(&button) {
        t.surface_hi
    } else {
        t.surface
    };
    cv.band(row, bg);
    let title = title_row(pane);
    let found = match b.result_rows.len() {
        0 => "no matches".to_owned(),
        1 => "1 pack".to_owned(),
        n => format!("{n} packs"),
    };
    let count = cv.line_right(
        title,
        row.y,
        &Line::styled(found, Style::new().fg(t.subtle)),
    );
    cv.text(title.x, row.y, "/", 1, Style::new().fg(t.accent).bold());
    let field = Rect::new(title.x + 2, row.y, count.x.saturating_sub(title.x + 4), 1);
    search.input.render(
        cv,
        field,
        Style::new().fg(t.strong),
        "Search every pack",
        search.typing,
    );
    cv.click(row, button);
}

/// A pack's row: its label, and its state on the right.
fn pack_row(
    cv: &mut Canvas<'_>,
    inner: Rect,
    row: &PackRow,
    (selected, hovered): (bool, bool),
    show_category: bool,
) {
    let t = *cv.theme();
    let accessory = state_line(&t, row, inner.width);
    let right = cv.line_right(inner, inner.y, &accessory);
    let mut style = widgets::row_label(&t, selected, hovered);
    if row.state == PackState::PendingRemoval {
        let fg = if selected { t.subtle } else { t.muted };
        style = style.fg(fg).add_modifier(Modifier::CROSSED_OUT);
    }
    let mut spans = text::highlight(&row.label, &row.positions, style, style.fg(t.accent));
    if show_category {
        spans.push(Span::styled(
            format!("  {}", row.category),
            Style::new().fg(t.faint),
        ));
    }
    let room = right.x.saturating_sub(inner.x + 2);
    cv.line(
        inner.x,
        inner.y,
        &text::fit(Line::from(spans), usize::from(room)),
        room,
    );
}

/// "● built", "● adding", "○ removing", or "⇄ replaces X" when picking the
/// pack would swap out a picked one.
fn state_line(t: &Theme, row: &PackRow, width: u16) -> Line<'static> {
    let styled = |s: String, fg| Line::styled(s, Style::new().fg(fg));
    match row.state {
        PackState::Built => styled(format!("{} built", ui::DOT), t.success),
        PackState::PendingAdd => styled(format!("{} adding", ui::DOT), t.warning),
        PackState::PendingRemoval => styled("○ removing".to_owned(), t.muted),
        PackState::Unpicked => match row.replaces.as_slice() {
            [] => Line::default(),
            [first, rest @ ..] => {
                let more = if rest.is_empty() {
                    String::new()
                } else {
                    format!(" +{}", rest.len())
                };
                // Never more than half the row, so the label keeps its room.
                let room = usize::from(width / 2).saturating_sub(11 + more.len());
                let name = text::truncate(first, room.max(4));
                styled(format!("⇄ replaces {name}{more}"), t.warning)
            }
        },
    }
}

/// The focused pack in full: its name, what it does, what it can't be used
/// with, and buttons for its preview and video.
fn details(cv: &mut Canvas<'_>, area: Rect, b: &Browser) {
    let t = *cv.theme();
    let Some((category, pack)) = b.focused_pack().and_then(|row| b.pack(&row.id)) else {
        return;
    };
    let left = area.x + 2;
    let width = area.width.saturating_sub(4);
    let mut y = area.y;

    let mut title = vec![Span::styled(pack.label(), Style::new().fg(t.strong).bold())];
    // Where it's from, when the pane doesn't say.
    if b.focus == Pane::Cart || b.search.is_some() {
        title.push(Span::styled(
            format!("  ·  {}", category.name),
            Style::new().fg(t.muted),
        ));
    }
    if pack.experiment {
        title.push(Span::styled(
            format!("  {} experimental", ui::WARN),
            Style::new().fg(t.warning),
        ));
    }
    cv.line(
        left,
        y,
        &text::fit(Line::from(title), usize::from(width)),
        width,
    );
    y += 1;

    let description = vanilla_tweaks::plain_text(&pack.description);
    if !description.is_empty() {
        for line in wrapped(&description, usize::from(width), 3) {
            cv.text(left, y, &line, width, Style::new().fg(t.subtle));
            y += 1;
        }
    }

    let incompatible: Vec<&str> = b
        .catalog
        .incompatible_with(&pack.name)
        .into_iter()
        .filter_map(|id| b.pack(id).map(|(_, p)| p.label()))
        .collect();
    if !incompatible.is_empty() {
        let line = listing("Not with ", &incompatible, usize::from(width));
        cv.text(left, y, &line, width, Style::new().fg(t.muted));
    }

    let buttons = area.bottom().saturating_sub(1);
    let end = widgets::button(
        cv,
        left,
        buttons,
        "Preview",
        Some("o"),
        ButtonKind::Secondary,
        Command::Preview.button(),
    )
    .right();
    if pack.video.is_some() {
        widgets::button(
            cv,
            end + 2,
            buttons,
            "Video",
            Some("w"),
            ButtonKind::Secondary,
            Command::Video.button(),
        );
    }
}

/// `lead` and as many `names` as fit in `width`, then "and N more".
fn listing(lead: &str, names: &[&str], width: usize) -> String {
    for shown in (1..=names.len()).rev() {
        let rest = names.len() - shown;
        let line = if rest == 0 {
            format!("{lead}{}", super::and_list(names))
        } else {
            format!("{lead}{} and {rest} more", names[..shown].join(", "))
        };
        if text::width(&line) <= width {
            return line;
        }
    }
    text::truncate(&format!("{lead}{}", names.join(", ")), width).into_owned()
}

fn cart(cv: &mut Canvas<'_>, pane: Rect, b: &Browser, mode: Mode, spin: Option<&str>) {
    let t = *cv.theme();
    cv.click(pane, target(PANE, Pane::Cart.key()));
    let focused = b.focus == Pane::Cart;
    if mode == Mode::Wide {
        let title = title_row(pane);
        // An empty pack says so on the right; a count would say it twice.
        let count = (!b.cart.is_empty()).then(|| b.cart.len().to_string());
        widgets::section(cv, title, "Your pack", count.as_deref(), focused);
        let end = text::width("YOUR PACK") + count.map_or(0, |n| n.len() + 3);
        cart_status(cv, title, title.x + end as u16, b);
    } else {
        tab_row(cv, pane, b, mode, Pane::Cart);
    }

    let (list, footer) = layout::cart_area(pane, cart_footer_height(b, pane.width));
    if b.cart_rows.is_empty() {
        let lines = [
            ("Nothing picked yet.", t.muted),
            (
                "Add packs with space, then build them into one with b.",
                t.faint,
            ),
        ];
        let mut y = list.y;
        for (copy, fg) in lines {
            for line in text::wrap(copy, inner_w(pane.width)) {
                cv.text(
                    list.x + 2,
                    y,
                    &line,
                    list.width.saturating_sub(4),
                    Style::new().fg(fg),
                );
                y += 1;
            }
            y += 1;
        }
    }
    b.basket.render(
        cv,
        list,
        focused,
        |cv, inner, i, selected, hovered| match b.cart_rows.get(i) {
            Some(CartRow::Group(name)) => {
                let fg = if hovered { t.subtle } else { t.dim };
                cv.text(
                    inner.x,
                    inner.y,
                    name,
                    inner.width,
                    Style::new().fg(fg).bold(),
                );
            }
            Some(CartRow::Pack(row)) => pack_row(cv, inner, row, (selected, hovered), false),
            None => {}
        },
    );
    if let Some(footer) = footer {
        cart_footer(cv, footer, b, spin);
    }
}

/// Your pack's state, flush right on `row` when it fits after `after`:
/// "● unbuilt changes", "✓ applied", "▲ not in use" or "empty".
fn cart_status(cv: &mut Canvas<'_>, row: Rect, after: u16, b: &Browser) {
    let t = *cv.theme();
    let (glyph, words, fg) = if b.cart.has_unbuilt_changes() {
        (ui::DOT, "unbuilt changes", t.warning)
    } else if b.cart.is_empty() {
        ("", "empty", t.muted)
    } else if !b.enabled {
        (ui::WARN, "not in use", t.warning)
    } else {
        (ui::CHECK, "applied", t.success)
    };
    // The words when there's room, else just the glyph.
    let full = if glyph.is_empty() {
        words.to_owned()
    } else {
        format!("{glyph} {words}")
    };
    let room = row.right().saturating_sub(after + 3);
    let status = [full.as_str(), glyph]
        .into_iter()
        .find(|s| !s.is_empty() && text::width(s) <= usize::from(room));
    if let Some(status) = status {
        cv.line_right(row, row.y, &Line::styled(status, Style::new().fg(fg)));
    }
}

/// Under your pack: why the game isn't using it, and the build button (or
/// the build under way).
fn cart_footer(cv: &mut Canvas<'_>, area: Rect, b: &Browser, spin: Option<&str>) {
    let t = *cv.theme();
    let left = area.x + 2;
    let width = area.width.saturating_sub(4);
    let mut y = area.y + 1;
    if let Some(note) = off_note(b) {
        for line in wrapped(note, usize::from(width), 2) {
            cv.text(left, y, &line, width, Style::new().fg(t.warning));
            y += 1;
        }
        y += 1;
    }
    match (&b.job, spin) {
        (Some(job), Some(spin)) => {
            let doing = match job.work {
                Work::Building => "Building your pack…",
                Work::Downloading => "Downloading…",
                Work::Removing => "Removing your pack…",
            };
            cv.text(left, y, spin, 1, Style::new().fg(t.accent));
            let room = width.saturating_sub(3);
            cv.text(left + 3, y, doing, room, Style::new().fg(t.subtle));
        }
        _ if b.needs_build() => {
            widgets::button(
                cv,
                left,
                y,
                "Build & apply",
                Some("b"),
                ButtonKind::Primary,
                Command::Build.button(),
            );
        }
        _ => {}
    }
}
