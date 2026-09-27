//! Drawing the wizard: the step chips across the top, the current step on
//! the left, and the choices so far on the right — or in one line at the
//! bottom when there isn't room for two panes.

use std::cmp::Ordering;
use std::time::Instant;

use crossterm::event::KeyCode;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use mctui_core::mojang::{VersionEntry, VersionKind};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::loaders::{FabricStatus, LoaderChoice, STARTER_MODS};
use super::versions::{Fetch, Filter};
use super::{Page, Step, Target, Wizard};
use crate::theme::Theme;
use crate::ui::form::{self, Form};
use crate::ui::input::TextInput;
use crate::ui::widgets::{self, ButtonKind, Tone};
use crate::ui::{self, Canvas, text};

/// At or above this body width the summary gets a pane of its own.
const SPLIT_MIN_WIDTH: u16 = 76;
/// The narrowest the name step's pane may be beside the summary: its form's
/// label column, a field, and the help in two lines.
const FORM_MIN_WIDTH: u16 = 60;
/// The summary pane's width range, and the step pane's widest.
const SIDE_MIN: u16 = 30;
const SIDE_MAX: u16 = 40;
const MAIN_MAX: u16 = 72;
/// Between the panes; each also insets its text by 2.
const GAP: u16 = 2;
/// The summary's label column.
const LABEL_W: u16 = 14;
/// The loader list: five loaders, each with a blank line after it.
const LOADER_ROWS: u16 = 10;

/// Where the wizard's parts go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Layout {
    /// The step chips.
    pub steps: Rect,
    /// The current step.
    pub main: Rect,
    /// The summary pane, when there's room.
    pub side: Option<Rect>,
    /// Otherwise the summary line, under the step (once there's a choice).
    pub line: Option<Rect>,
}

/// Where things go on `step`. The name step's form needs more room than a
/// list, so it gives up the summary pane sooner.
pub(super) fn layout(body: Rect, step: Step) -> Layout {
    let steps = Rect {
        height: body.height.min(1),
        ..body
    };
    let top = body.y + body.height.min(2);
    let height = body.bottom() - top;
    // u32: `width * 34` overflows u16 on very wide terminals.
    let side_w = (u32::from(body.width) * 34 / 100).clamp(SIDE_MIN.into(), SIDE_MAX.into()) as u16;
    let main_w = body.width.saturating_sub(side_w + GAP).min(MAIN_MAX);
    let fits = match step {
        Step::Version | Step::Loader => body.width >= SPLIT_MIN_WIDTH,
        Step::Name => main_w >= FORM_MIN_WIDTH,
    };
    if !fits {
        // Nothing's chosen yet on the first step, so it has no summary.
        let line = (height >= 3 && step > Step::Version).then(|| {
            Rect::new(
                body.x + 2,
                body.bottom() - 1,
                body.width.saturating_sub(4),
                1,
            )
        });
        let main_h = if line.is_some() { height - 2 } else { height };
        return Layout {
            steps,
            main: Rect::new(body.x, top, body.width.min(MAIN_MAX), main_h),
            side: None,
            line,
        };
    }
    Layout {
        steps,
        main: Rect::new(body.x, top, main_w, height),
        side: Some(Rect::new(body.x + main_w + GAP, top, side_w, height)),
        line: None,
    }
}

/// Where the version rows go: under the heading, the offline note when
/// there is one, and a blank line.
pub(super) fn version_rows(main: Rect, note: bool) -> Rect {
    rows_below(main, 2 + u16::from(note))
}

/// Where the loader rows go.
pub(super) fn loader_rows(main: Rect) -> Rect {
    let rows = rows_below(main, 2);
    Rect {
        height: rows.height.min(LOADER_ROWS),
        ..rows
    }
}

fn rows_below(main: Rect, top: u16) -> Rect {
    let top = top.min(main.height);
    Rect {
        y: main.y + top,
        height: main.height - top,
        ..main
    }
}

/// The heading row of a pane, inset like its rows' text.
fn heading(pane: Rect) -> Rect {
    Rect::new(
        pane.x + 2,
        pane.y,
        pane.width.saturating_sub(4),
        1.min(pane.height),
    )
}

/// The choices so far; the current step's highlight stands in for its own.
struct Summary {
    version: Option<String>,
    /// e.g. "Fabric 0.19.5".
    loader: Option<String>,
    name: Option<String>,
    /// Whether the starter mods come along; `None` unless it's Fabric.
    starter: Option<bool>,
}

impl Wizard {
    pub(super) fn draw(&self, cv: &mut Canvas<'_>) {
        let lay = layout(self.body, self.step());
        self.render_steps(cv, lay.steps);
        cv.clipped(lay.main, |cv| {
            cv.click(lay.main, ui::Target::Wizard(Target::Pane));
            match &self.page {
                Page::Version => self.render_versions(cv, lay.main),
                Page::Loader => self.render_loaders(cv, lay.main),
                Page::Name(form) => self.render_name(cv, lay.main, form),
            }
        });
        if let Some(side) = lay.side {
            cv.clipped(side, |cv| self.render_summary(cv, side));
        }
        if let Some(area) = lay.line {
            let t = *cv.theme();
            let line = text::fit(self.summary_line(&t), usize::from(area.width));
            cv.line(area.x, area.y, &line, area.width);
        }
    }

    /// `✓ Version  ›  Loader  ›  Name`: the current step filled, finished
    /// ones clickable, the rest quiet.
    fn render_steps(&self, cv: &mut Canvas<'_>, row: Rect) {
        let t = *cv.theme();
        let current = self.step();
        // The chips' padding hangs left so their text lines up with the panes'.
        let mut x = row.x + 1;
        for (i, step) in Step::ALL.into_iter().enumerate() {
            if i > 0 {
                x = cv.text(x + 1, row.y, "›", 1, Style::new().fg(t.faint)) + 1;
            }
            let room = row.right().saturating_sub(x);
            if room < 4 {
                break;
            }
            let (line, tone, target) = match step.cmp(&current) {
                Ordering::Less => {
                    let target = ui::Target::Wizard(Target::Step(step));
                    let fg = if cv.hovered(&target) { t.text } else { t.dim };
                    let line = Line::from(vec![
                        Span::styled(format!("{} ", ui::CHECK), Style::new().fg(t.success)),
                        Span::styled(step.title(), Style::new().fg(fg)),
                    ]);
                    (line, Tone::Plain, Some(target))
                }
                Ordering::Equal => (
                    Line::styled(step.title(), Style::new().fg(t.strong).bold()),
                    Tone::Selected,
                    None,
                ),
                Ordering::Greater => (
                    Line::styled(step.title(), Style::new().fg(t.muted)),
                    Tone::Plain,
                    None,
                ),
            };
            x = widgets::chip(cv, x, row.y, &line, tone, target, room).right();
        }
    }

    fn render_versions(&self, cv: &mut Canvas<'_>, pane: Rect) {
        let t = *cv.theme();
        let versions = &self.versions;
        let Some(manifest) = &versions.manifest else {
            let label = list_label(versions.all);
            widgets::section(cv, heading(pane), label, None, true);
            let rows = version_rows(pane, false);
            let at = Rect::new(
                rows.x + 2,
                rows.y,
                rows.width.saturating_sub(4),
                rows.height,
            );
            match versions.fetch {
                Fetch::Failed => failed(cv, at),
                Fetch::Running { started, .. } => {
                    self.spinner(cv, at, started, "Loading Minecraft versions…");
                }
                Fetch::Idle => {}
            }
            return;
        };
        self.version_heading(cv, pane);

        let rows = version_rows(pane, versions.stale);
        if versions.rows.is_empty() {
            let empty = if versions.all {
                "No versions match."
            } else {
                "No releases match — space shows snapshots too."
            };
            let style = Style::new().fg(t.faint);
            cv.text(
                rows.x + 2,
                rows.y,
                empty,
                rows.width.saturating_sub(4),
                style,
            );
            return;
        }
        let latest = &manifest.latest;
        versions
            .list
            .render(cv, rows, true, |cv, inner, i, selected, hovered| {
                let Some(row) = versions.rows.get(i) else {
                    return;
                };
                let Some(entry) = manifest.versions.get(row.index) else {
                    return;
                };
                let is_latest =
                    entry.id == latest.release || (versions.all && entry.id == latest.snapshot);
                let state = RowState { selected, hovered };
                version_row(cv, inner, entry, &row.positions, is_latest, state);
            });
    }

    /// The version list's heading — the filter field while one is typed —
    /// and, when the list is the saved copy, a note saying so.
    fn version_heading(&self, cv: &mut Canvas<'_>, pane: Rect) {
        let t = *cv.theme();
        let versions = &self.versions;
        let head = heading(pane);
        let total = versions.total();
        let shown = versions.rows.len();
        let count = if shown == total {
            total.to_string()
        } else {
            format!("{shown} of {total}")
        };
        let count = match &versions.filter {
            Filter::Editing(input) => {
                filter_field(cv, pane, head, input, &count);
                None
            }
            Filter::Applied(input) => {
                let query = text::truncate(input.as_str(), 16);
                Some(format!("{count}   “{query}”"))
            }
            Filter::Off => Some(count),
        };
        if let Some(count) = count {
            let label = list_label(versions.all);
            widgets::section(cv, head, label, Some(&count), true);
            let after = head.x + (text::width(label) + 3 + text::width(&count)) as u16;
            snapshot_toggle(cv, head, after, versions.all);
        }

        if versions.stale {
            let note = Rect {
                y: head.y + 1,
                ..head
            };
            match versions.fetch {
                Fetch::Running { started, .. } => {
                    self.spinner(cv, note, started, "Reconnecting to Mojang…");
                }
                Fetch::Idle | Fetch::Failed => {
                    let copy = "Offline — showing saved versions";
                    cv.text(note.x, note.y, copy, note.width, Style::new().fg(t.muted));
                }
            }
        }
    }

    fn render_loaders(&self, cv: &mut Canvas<'_>, pane: Rect) {
        let t = *cv.theme();
        widgets::section(cv, heading(pane), "Mod loader", None, true);
        let rows = loader_rows(pane);
        self.loaders
            .render(cv, rows, true, |cv, inner, i, selected, hovered| {
                if let Some(choice) = LoaderChoice::at(i) {
                    self.loader_row(cv, inner, choice, RowState { selected, hovered });
                }
            });
        // What the highlighted loader is, a blank line under the list.
        let y = rows.y + LOADER_ROWS;
        let Some(choice) = self.highlighted_loader() else {
            return;
        };
        if rows.height < LOADER_ROWS || y >= pane.bottom() {
            return;
        }
        let w = pane.width.saturating_sub(4);
        for (i, line) in text::wrap(&self.blurb(choice), usize::from(w))
            .iter()
            .take(2)
            .enumerate()
        {
            cv.text(pane.x + 2, y + i as u16, line, w, Style::new().fg(t.muted));
        }
    }

    fn loader_row(&self, cv: &mut Canvas<'_>, inner: Rect, choice: LoaderChoice, row: RowState) {
        let t = *cv.theme();
        let selectable = self.selectable(choice);
        let name_style = if selectable {
            widgets::row_label(&t, row.selected, row.hovered)
        } else {
            Style::new().fg(t.faint)
        };
        let quiet = Style::new().fg(if row.selected { t.subtle } else { t.dim });
        let right = match (choice, self.fabric_status()) {
            (LoaderChoice::Fabric, Some(FabricStatus::Checking { started, .. })) => Line::styled(
                widgets::spinner(*started, self.now),
                Style::new().fg(t.accent),
            ),
            (LoaderChoice::Fabric, Some(FabricStatus::Ready(version))) => {
                Line::styled(version.clone(), quiet)
            }
            (LoaderChoice::Fabric, Some(FabricStatus::Unsupported)) => Line::styled(
                format!("Not available for {}", self.version),
                Style::new().fg(t.muted),
            ),
            (LoaderChoice::Fabric, Some(FabricStatus::Failed)) => {
                Line::styled("Couldn't reach Fabric", Style::new().fg(t.warning))
            }
            (LoaderChoice::Fabric | LoaderChoice::Vanilla, _) => Line::default(),
            _ => Line::styled("coming soon", Style::new().fg(t.faint)),
        };
        let name_w = text::width(choice.name()) as u16;
        let right = text::fit(right, usize::from(inner.width.saturating_sub(name_w + 2)));
        let right_x = cv.line_right(inner, inner.y, &right).x;

        let mut spans = vec![Span::styled(choice.name(), name_style)];
        if choice == LoaderChoice::Fabric && selectable {
            spans.push(Span::styled("  recommended", Style::new().fg(t.secondary)));
        }
        let room = right_x.saturating_sub(inner.x + 2);
        cv.line(
            inner.x,
            inner.y,
            &text::fit(Line::from(spans), usize::from(room)),
            room,
        );
    }

    /// A sentence about a loader for this version.
    fn blurb(&self, choice: LoaderChoice) -> String {
        match (choice, self.fabric_status()) {
            (LoaderChoice::Fabric, Some(FabricStatus::Ready(_))) => {
                "Fast and light, with the most mods. Browse and install them in mctui.".to_owned()
            }
            (LoaderChoice::Fabric, Some(FabricStatus::Unsupported)) => {
                format!("Fabric doesn't support Minecraft {} yet.", self.version)
            }
            (LoaderChoice::Fabric, Some(FabricStatus::Failed)) => {
                "Check your connection, then press r to try again.".to_owned()
            }
            (LoaderChoice::Fabric, _) => {
                format!("Checking Fabric for Minecraft {}…", self.version)
            }
            (LoaderChoice::Vanilla, _) => {
                "The game exactly as Mojang ships it, no mods.".to_owned()
            }
            (other, _) => format!("{} support is coming soon.", other.name()),
        }
    }

    fn render_name(&self, cv: &mut Canvas<'_>, pane: Rect, form: &Form) {
        widgets::section(cv, heading(pane), "Your instance", None, true);
        let area = rows_below(pane, 2);
        let area = Rect {
            x: area.x + 2,
            width: area.width.saturating_sub(4),
            ..area
        };
        form.render(cv, area, true, ui::Target::Field);

        // The button, a blank line under the fields and the focused one's help.
        let fields: u16 = if self.loader == LoaderChoice::Fabric {
            2
        } else {
            1
        };
        let help = form.field(form.id(form.focus())).help;
        let help_w = area.right().saturating_sub(area.x + form::LABEL_W + 1);
        let help_h = if help.is_empty() {
            0
        } else {
            text::wrap(help, usize::from(help_w)).len().min(2) as u16
        };
        let y = area.y + fields * 2 + if help_h > 0 { help_h + 1 } else { 0 };
        if y < area.bottom() {
            widgets::button(
                cv,
                area.x + form::LABEL_W,
                y,
                "Create instance",
                Some("↵"),
                ButtonKind::Primary,
                ui::Target::Key(KeyCode::Enter),
            );
        }
    }

    fn summary(&self) -> Summary {
        match &self.page {
            Page::Version => Summary {
                version: self.versions.selected().map(|e| e.id.clone()),
                loader: None,
                name: None,
                starter: None,
            },
            Page::Loader => Summary {
                version: Some(self.version.clone()),
                loader: self
                    .highlighted_loader()
                    .filter(|c| self.selectable(*c))
                    .map(|c| loader_label(c, self.usable(c).unwrap_or_default())),
                name: None,
                starter: None,
            },
            Page::Name(form) => Summary {
                version: Some(self.version.clone()),
                loader: Some(loader_label(self.loader, &self.loader_version)),
                name: Some(form.text("name").trim().to_owned()).filter(|n| !n.is_empty()),
                starter: (self.loader == LoaderChoice::Fabric).then(|| form.toggle("starter")),
            },
        }
    }

    /// The summary pane: each choice on a label/value row.
    fn render_summary(&self, cv: &mut Canvas<'_>, pane: Rect) {
        let t = *cv.theme();
        let x = pane.x + 2;
        let w = pane.width.saturating_sub(2);
        widgets::section(cv, Rect::new(x, pane.y, w, 1), "Summary", None, false);
        let summary = self.summary();
        let value = |v: Option<String>| match v {
            Some(v) => Line::styled(v, Style::new().fg(t.text)),
            None => Line::styled("—", Style::new().fg(t.faint)),
        };
        let mut rows = vec![
            ("Minecraft", value(summary.version)),
            ("Loader", value(summary.loader)),
            ("Name", value(summary.name)),
        ];
        // The starter mods, one under another.
        match summary.starter {
            Some(true) => rows.extend(STARTER_MODS.iter().enumerate().map(|(i, name)| {
                let label = if i == 0 { "Starter mods" } else { "" };
                (label, Line::styled(*name, Style::new().fg(t.text)))
            })),
            Some(false) => {
                let none = Line::styled("None", Style::new().fg(t.dim));
                rows.push(("Starter mods", none));
            }
            None => {}
        }
        for (i, (label, value)) in rows.iter().enumerate() {
            let row = Rect::new(x, pane.y + 2 + i as u16, w, 1);
            widgets::kv(cv, row, label, value, LABEL_W);
        }
    }

    /// The narrow layout's summary: "Minecraft 1.21.4 · Fabric 0.16.10 ·
    /// starter mods".
    fn summary_line(&self, t: &Theme) -> Line<'static> {
        let summary = self.summary();
        let sep = || Span::styled(" · ", Style::new().fg(t.muted));
        let mut spans = Vec::new();
        if let Some(version) = summary.version {
            spans.push(Span::styled("Minecraft ", Style::new().fg(t.dim)));
            spans.push(Span::styled(version, Style::new().fg(t.text)));
        }
        if let Some(loader) = summary.loader {
            spans.extend([sep(), Span::styled(loader, Style::new().fg(t.text))]);
        }
        if summary.starter == Some(true) {
            spans.extend([sep(), Span::styled("starter mods", Style::new().fg(t.dim))]);
        }
        Line::from(spans)
    }

    /// A spinner and what's being waited for.
    fn spinner(&self, cv: &mut Canvas<'_>, at: Rect, started: Instant, what: &str) {
        let t = *cv.theme();
        let frame = widgets::spinner(started, self.now);
        cv.text(at.x, at.y, frame, 1, Style::new().fg(t.accent));
        let w = at.width.saturating_sub(2);
        cv.text(at.x + 2, at.y, what, w, Style::new().fg(t.subtle));
    }
}

/// How a list row is lit.
#[derive(Debug, Clone, Copy)]
struct RowState {
    selected: bool,
    hovered: bool,
}

/// One version: its id (filter matches lit) and kind on the left; a
/// "latest" chip or its release date on the right.
fn version_row(
    cv: &mut Canvas<'_>,
    inner: Rect,
    entry: &VersionEntry,
    positions: &[u32],
    latest: bool,
    row: RowState,
) {
    let t = *cv.theme();
    let right_x = if latest {
        let line = Line::styled("latest", Style::new().fg(t.success));
        let w = line.width() as u16 + 2;
        let x = inner.right().saturating_sub(w).max(inner.x);
        widgets::chip(cv, x, inner.y, &line, Tone::Success, None, w).x
    } else {
        let fg = if row.selected { t.subtle } else { t.dim };
        let date = release_date(entry.release_time, &cv.cx.tz);
        cv.line_right(inner, inner.y, &Line::styled(date, Style::new().fg(fg)))
            .x
    };
    let style = widgets::row_label(&t, row.selected, row.hovered);
    let mut spans = text::highlight(&entry.id, positions, style, style.fg(t.accent));
    if let Some(kind) = kind_label(&entry.kind) {
        spans.push(Span::styled(format!("  {kind}"), Style::new().fg(t.muted)));
    }
    let room = right_x.saturating_sub(inner.x + 2);
    cv.line(
        inner.x,
        inner.y,
        &text::fit(Line::from(spans), usize::from(room)),
        room,
    );
}

/// The version list's heading: what it lists.
fn list_label(all: bool) -> &'static str {
    if all { "All versions" } else { "Releases" }
}

/// A quiet word for versions that aren't releases.
fn kind_label(kind: &VersionKind) -> Option<&str> {
    match kind {
        VersionKind::Release => None,
        VersionKind::Snapshot => Some("snapshot"),
        VersionKind::OldBeta => Some("beta"),
        VersionKind::OldAlpha => Some("alpha"),
        VersionKind::Other(kind) => Some(kind),
    }
}

/// "Jun 17, 2025".
fn release_date(at: Timestamp, tz: &TimeZone) -> String {
    at.to_zoned(tz.clone()).strftime("%b %-d, %Y").to_string()
}

/// "Fabric 0.19.5", or just "Vanilla".
fn loader_label(choice: LoaderChoice, version: &str) -> String {
    if version.is_empty() {
        choice.name().to_owned()
    } else {
        format!("{} {version}", choice.name())
    }
}

/// The versions couldn't be had at all: say so, and offer to try again.
fn failed(cv: &mut Canvas<'_>, at: Rect) {
    let t = *cv.theme();
    let headline = format!("{} Couldn't reach Mojang", ui::CROSS);
    cv.text(at.x, at.y, &headline, at.width, Style::new().fg(t.error));
    let detail = "Check your connection, then press r to try again.";
    cv.text(at.x, at.y + 1, detail, at.width, Style::new().fg(t.dim));
    let retry = ui::Target::Key(KeyCode::Char('r'));
    widgets::button(
        cv,
        at.x,
        at.y + 3,
        "Try again",
        Some("r"),
        ButtonKind::Primary,
        retry,
    );
}

/// The heading row as a filled filter field, the count flush right.
fn filter_field(cv: &mut Canvas<'_>, pane: Rect, head: Rect, input: &TextInput, count: &str) {
    let t = *cv.theme();
    cv.band(Rect::new(pane.x, head.y, pane.width, 1), t.selection);
    let count = Line::styled(count.to_owned(), Style::new().fg(t.subtle));
    let right = cv.line_right(head, head.y, &count);
    cv.text(head.x, head.y, "/", 1, Style::new().fg(t.accent).bold());
    let field = Rect::new(head.x + 2, head.y, right.x.saturating_sub(head.x + 4), 1);
    input.render(
        cv,
        field,
        Style::new().fg(t.strong),
        "filter versions",
        true,
    );
}

/// `Snapshots ○ Off` flush right on the heading; clicking it presses space.
/// Left out when it would crowd the heading's text, which ends at `after`.
fn snapshot_toggle(cv: &mut Canvas<'_>, head: Rect, after: u16, on: bool) {
    const LABEL: &str = "Snapshots";
    let t = *cv.theme();
    let label_w = text::width(LABEL) as u16;
    // " ● On " or " ○ Off ".
    let chip_w: u16 = if on { 6 } else { 7 };
    let w = label_w + 2 + chip_w;
    if head.right() < after + 4 + w {
        return;
    }
    let x = head.right() - w;
    let target = ui::Target::Key(KeyCode::Char(' '));
    let fg = if cv.hovered(&target) { t.text } else { t.muted };
    cv.text(x, head.y, LABEL, label_w, Style::new().fg(fg));
    cv.click(Rect::new(x, head.y, label_w + 2, 1), target.clone());
    widgets::toggle(cv, x + label_w + 2, head.y, on, false, target);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_the_breakpoint_and_caps_the_step_pane() {
        let narrow = layout(Rect::new(2, 3, 75, 20), Step::Loader);
        assert!(narrow.side.is_none());
        assert_eq!(narrow.line.map(|l| l.y), Some(22));
        assert_eq!(narrow.main.height, 16);
        let first = layout(Rect::new(2, 3, 75, 20), Step::Version);
        assert_eq!((first.line, first.main.height), (None, 18));

        let split = layout(Rect::new(2, 3, 76, 20), Step::Loader);
        let side = split.side.unwrap_or_default();
        assert_eq!((split.main.width, side.width), (44, 30));
        assert_eq!(side.x, 2 + 44 + GAP);

        let wide = layout(Rect::new(0, 0, 300, 40), Step::Version);
        assert_eq!(wide.main.width, MAIN_MAX);
        assert_eq!(wide.side.unwrap_or_default().width, SIDE_MAX);
    }

    #[test]
    fn the_form_keeps_its_room() {
        let body = |w| Rect::new(2, 3, w, 20);
        assert!(layout(body(76), Step::Name).side.is_none());
        assert!(layout(body(96), Step::Name).side.is_some());
    }

    #[test]
    fn tiny_bodies_do_not_underflow() {
        for (w, h) in [(0, 0), (1, 1), (80, 1), (80, 2), (10, 3)] {
            for step in Step::ALL {
                let lay = layout(Rect::new(0, 0, w, h), step);
                let _ = (version_rows(lay.main, true), loader_rows(lay.main));
            }
        }
    }

    #[test]
    fn dates_and_kinds() {
        let at: Timestamp = "2025-06-17T10:00:00Z".parse().unwrap_or_default();
        assert_eq!(release_date(at, &TimeZone::UTC), "Jun 17, 2025");
        assert_eq!(kind_label(&VersionKind::Release), None);
        assert_eq!(kind_label(&VersionKind::OldBeta), Some("beta"));
    }
}
