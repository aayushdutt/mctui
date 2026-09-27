//! The components every screen is built from (see DESIGN.md). Each draws one
//! row at most, registers its click target, and lights up under the pointer.

use std::time::{Duration, Instant};

use mctui_core::net::Progress;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::input::TextInput;
use super::{Canvas, Target, text};
use crate::theme;

/// An uppercase section label with an optional count: `INSTANCES   6`.
/// Accent while its pane has focus. Returns the x after what it drew.
pub fn section(
    cv: &mut Canvas<'_>,
    area: Rect,
    label: &str,
    count: Option<&str>,
    focused: bool,
) -> u16 {
    let t = *cv.theme();
    let fg = if focused { t.accent } else { t.muted };
    let label = label.to_uppercase();
    let end = cv.text(
        area.x,
        area.y,
        &label,
        area.width,
        Style::new().fg(fg).bold(),
    );
    match count {
        Some(count) => {
            let room = area.right().saturating_sub(end + 3);
            cv.text(end + 3, area.y, count, room, Style::new().fg(t.muted))
        }
        None => end,
    }
}

/// Paints a list row's band — the selection when `selected`, a hover tint when
/// `hovered` — and returns the area its content goes in (inset 2 columns).
pub fn row(cv: &mut Canvas<'_>, area: Rect, selected: bool, hovered: bool) -> Rect {
    let t = *cv.theme();
    if selected {
        cv.band(area, t.selection);
    } else if hovered {
        cv.band(area, t.surface);
    }
    Rect {
        x: area.x + 2,
        width: area.width.saturating_sub(4),
        height: 1,
        ..area
    }
}

/// The text style for a row's main label.
pub fn row_label(theme: &theme::Theme, selected: bool, hovered: bool) -> Style {
    match (selected, hovered) {
        (true, _) => Style::new().fg(theme.strong).bold(),
        (false, true) => Style::new().fg(theme.strong),
        (false, false) => Style::new().fg(theme.text),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// The main action: filled with the accent.
    Primary,
    /// Everything else: a quiet surface.
    Secondary,
    /// Destructive and chosen: filled red.
    Danger,
}

/// A clickable button: ` label  key `. Returns its area.
pub fn button(
    cv: &mut Canvas<'_>,
    x: u16,
    y: u16,
    label: &str,
    key: Option<&str>,
    kind: ButtonKind,
    target: Target,
) -> Rect {
    let t = *cv.theme();
    let hovered = cv.hovered(&target);
    let bg = match (kind, hovered) {
        (ButtonKind::Primary, false) => t.accent,
        (ButtonKind::Primary, true) => t.accent_hi,
        (ButtonKind::Danger, _) => t.error,
        (ButtonKind::Secondary, false) => t.surface,
        (ButtonKind::Secondary, true) => t.surface_hi,
    };
    let fg = match kind {
        ButtonKind::Secondary if hovered => t.strong,
        ButtonKind::Secondary => t.text,
        _ => theme::on_color(bg),
    };
    let label_style = match kind {
        ButtonKind::Secondary => Style::new().fg(fg),
        _ => Style::new().fg(fg).bold(),
    };
    let mut spans = vec![Span::raw(" "), Span::styled(label, label_style)];
    if let Some(key) = key {
        spans.push(Span::styled(format!("  {key}"), Style::new().fg(fg)));
    }
    spans.push(Span::raw(" "));
    let area = cv.pill(x, y, &Line::from(spans), bg, 60);
    cv.click(area, target);
    area
}

/// What a chip's tint says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Inactive: text only, a surface under the pointer.
    Plain,
    Neutral,
    /// Active, in a focused pane.
    Selected,
    Success,
    Warning,
}

/// A one-row chip. Clickable when given a target. Returns its area.
pub fn chip(
    cv: &mut Canvas<'_>,
    x: u16,
    y: u16,
    line: &Line<'_>,
    tone: Tone,
    target: Option<Target>,
    max: u16,
) -> Rect {
    let t = *cv.theme();
    let hovered = target.as_ref().is_some_and(|target| cv.hovered(target));
    let bg = match tone {
        Tone::Plain if hovered => t.surface,
        Tone::Plain => Color::Reset,
        Tone::Neutral if hovered => t.surface_hi,
        Tone::Neutral => t.surface,
        Tone::Selected => t.selection,
        Tone::Success => t.success_bg,
        Tone::Warning => t.warning_bg,
    };
    let area = cv.pill(x, y, line, bg, max);
    if let Some(target) = target {
        cv.click(area, target);
    }
    area
}

/// A row of tab chips; the active one filled (with the selection tint while
/// the pane has focus). Each tab is `(label, count)`.
pub fn tabs(
    cv: &mut Canvas<'_>,
    area: Rect,
    tabs: &[(&str, Option<usize>)],
    active: usize,
    focused: bool,
    target: impl Fn(usize) -> Target,
) {
    let t = *cv.theme();
    let mut x = area.x;
    for (i, (label, count)) in tabs.iter().enumerate() {
        let room = area.right().saturating_sub(x);
        if room < 4 {
            break;
        }
        let is_active = i == active;
        let hovered = cv.hovered(&target(i));
        let fg = match (is_active, hovered) {
            (true, _) => t.strong,
            (false, true) => t.text,
            (false, false) => t.dim,
        };
        let style = if is_active {
            Style::new().fg(fg).bold()
        } else {
            Style::new().fg(fg)
        };
        let mut spans = vec![Span::styled(*label, style)];
        if let Some(n) = count {
            let fg = if is_active { t.subtle } else { t.muted };
            spans.push(Span::styled(format!(" {n}"), Style::new().fg(fg)));
        }
        let tone = match (is_active, focused) {
            (true, true) => Tone::Selected,
            (true, false) => Tone::Neutral,
            (false, _) => Tone::Plain,
        };
        x = chip(
            cv,
            x,
            area.y,
            &Line::from(spans),
            tone,
            Some(target(i)),
            room,
        )
        .right()
            + 1;
    }
}

/// A field's band: the selection while focused, brighter under the pointer.
fn field_bg(t: &theme::Theme, focused: bool, hovered: bool) -> Color {
    match (focused, hovered) {
        (true, _) => t.selection,
        (false, true) => t.surface_hi,
        (false, false) => t.surface,
    }
}

/// A one-row text field: the text on a filled band, the cursor when focused.
pub fn field(
    cv: &mut Canvas<'_>,
    area: Rect,
    input: &TextInput,
    placeholder: &str,
    focused: bool,
    target: Option<Target>,
) {
    let t = *cv.theme();
    let hovered = target.as_ref().is_some_and(|target| cv.hovered(target));
    let area = Rect { height: 1, ..area };
    cv.band(area, field_bg(&t, focused, hovered));
    let inner = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..area
    };
    input.render(cv, inner, Style::new().fg(t.strong), placeholder, focused);
    if let Some(target) = target {
        cv.click(area, target);
    }
}

/// A search field filling `row` (a pane's width): its key `/`, the query, and
/// a count flush right, the text inset 2 columns like the pane's rows. Quiet
/// until focused.
pub fn search(
    cv: &mut Canvas<'_>,
    row: Rect,
    input: &TextInput,
    placeholder: &str,
    count: Option<&str>,
    focused: bool,
    target: Option<Target>,
) {
    let t = *cv.theme();
    let row = Rect {
        height: row.height.min(1),
        ..row
    };
    let hovered = target.as_ref().is_some_and(|target| cv.hovered(target));
    cv.band(row, field_bg(&t, focused, hovered));
    let inner = Rect::new(row.x + 2, row.y, row.width.saturating_sub(4), 1);
    // The query stops 2 columns short of the count.
    let end = match count {
        Some(count) => {
            let count = Line::styled(count, Style::new().fg(t.subtle));
            cv.line_right(inner, inner.y, &count).x.saturating_sub(2)
        }
        None => inner.right(),
    };
    let key = if focused {
        Style::new().fg(t.accent).bold()
    } else {
        Style::new().fg(t.muted)
    };
    cv.text(inner.x, inner.y, "/", 1, key);
    let field = Rect::new(inner.x + 2, inner.y, end.saturating_sub(inner.x + 2), 1);
    let style = Style::new().fg(if focused { t.strong } else { t.text });
    input.render(cv, field, style, placeholder, focused);
    if let Some(target) = target {
        cv.click(row, target);
    }
}

/// An on/off switch: ` ● On ` (green) or ` ○ Off `. Returns its area.
pub fn toggle(
    cv: &mut Canvas<'_>,
    x: u16,
    y: u16,
    on: bool,
    focused: bool,
    target: Target,
) -> Rect {
    let t = *cv.theme();
    let (text, fg, tone) = if on {
        ("● On", t.success, Tone::Success)
    } else {
        ("○ Off", t.dim, Tone::Neutral)
    };
    let tone = if focused && !on { Tone::Selected } else { tone };
    let style = if focused {
        Style::new().fg(fg).bold()
    } else {
        Style::new().fg(fg)
    };
    chip(cv, x, y, &Line::styled(text, style), tone, Some(target), 12)
}

/// A value you cycle through: ` ‹ catppuccin › `. Returns its area.
pub fn choice(
    cv: &mut Canvas<'_>,
    x: u16,
    y: u16,
    value: &str,
    focused: bool,
    target: Target,
) -> Rect {
    let t = *cv.theme();
    let arrows = Style::new().fg(if focused { t.accent } else { t.muted });
    let value_style = if focused {
        Style::new().fg(t.strong).bold()
    } else {
        Style::new().fg(t.text)
    };
    let line = Line::from(vec![
        Span::styled("‹ ", arrows),
        Span::styled(value, value_style),
        Span::styled(" ›", arrows),
    ]);
    let tone = if focused {
        Tone::Selected
    } else {
        Tone::Neutral
    };
    chip(cv, x, y, &line, tone, Some(target), 40)
}

/// A one-row progress bar: `━━━━━━━──────  45%`.
pub fn progress(cv: &mut Canvas<'_>, area: Rect, ratio: f64) {
    let t = *cv.theme();
    let ratio = ratio.clamp(0.0, 1.0);
    let label = format!("{:>4}%", (ratio * 100.0).round());
    let bar_w = area.width.saturating_sub(text::width(&label) as u16 + 2);
    #[expect(clippy::cast_sign_loss, reason = "ratio is clamped to 0..=1")]
    let filled = (f64::from(bar_w) * ratio).round() as u16;
    let bar = Line::from(vec![
        Span::styled("━".repeat(usize::from(filled)), Style::new().fg(t.accent)),
        Span::styled(
            "─".repeat(usize::from(bar_w - filled)),
            Style::new().fg(t.rule),
        ),
    ]);
    cv.line(area.x, area.y, &bar, bar_w);
    cv.text(
        area.x + bar_w + 2,
        area.y,
        &label,
        area.width,
        Style::new().fg(t.subtle),
    );
}

/// How far along a download is, for [`progress`]: by bytes when their total
/// is known, else by files.
pub fn download_ratio(p: Progress) -> f64 {
    if p.bytes_total > 0 {
        p.bytes_done as f64 / p.bytes_total as f64
    } else {
        p.files_done as f64 / p.files_total.max(1) as f64
    }
}

/// How often the spinner advances.
pub const SPIN: Duration = Duration::from_millis(80);

/// The spinner frame for work that began at `started`.
pub fn spinner(started: Instant, now: Instant) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let n = now.saturating_duration_since(started).as_millis() / SPIN.as_millis();
    FRAMES[(n % FRAMES.len() as u128) as usize]
}

/// The spinner frame for a frame drawn at `now` (the frame's own clock, so
/// renders are repeatable).
pub fn spinner_at(now: jiff::Timestamp) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let n = now.as_millisecond().unsigned_abs() / SPIN.as_millis() as u64;
    FRAMES[(n % FRAMES.len() as u64) as usize]
}

/// When a [`spinner_at`] next changes frame, given the wall clock and the
/// monotonic clock at the same moment.
pub fn next_spin_at(wall: jiff::Timestamp, now: Instant) -> Instant {
    let step = SPIN.as_millis() as u64;
    let into = wall.as_millisecond().unsigned_abs() % step;
    now + Duration::from_millis(step - into)
}

/// The next time a spinner that began at `started` changes frame.
pub fn next_spin(started: Instant, now: Instant) -> Instant {
    let elapsed = now.saturating_duration_since(started).as_millis();
    let step = SPIN.as_millis();
    started + Duration::from_millis(((elapsed / step + 1) * step) as u64)
}

/// A label/value row: the label dim in a fixed column, then the value.
pub fn kv(cv: &mut Canvas<'_>, area: Rect, label: &str, value: &Line<'_>, label_w: u16) {
    let t = *cv.theme();
    cv.text(area.x, area.y, label, label_w, Style::new().fg(t.dim));
    let w = area.width.saturating_sub(label_w);
    cv.line(
        area.x + label_w,
        area.y,
        &text::fit(value.clone(), usize::from(w)),
        w,
    );
}

/// Stat tiles: each value bold over its label, sharing the width and wrapping
/// into rows when narrow. Returns the rows used.
pub fn stats(cv: &mut Canvas<'_>, area: Rect, stats: &[(String, &str)]) -> u16 {
    const MIN_TILE: u16 = 13;
    let t = *cv.theme();
    if stats.is_empty() || area.width == 0 {
        return 0;
    }
    let per_row = (area.width / MIN_TILE).clamp(1, stats.len() as u16);
    let tile_w = area.width / per_row;
    let mut y = area.y;
    for chunk in stats.chunks(usize::from(per_row)) {
        for (i, (value, label)) in chunk.iter().enumerate() {
            let x = area.x + i as u16 * tile_w;
            let w = tile_w.saturating_sub(1);
            cv.text(x, y, value, w, Style::new().fg(t.strong).bold());
            cv.text(x, y + 1, label, w, Style::new().fg(t.muted));
        }
        y += 3;
    }
    y - area.y - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_cycles_and_schedules_the_next_frame() {
        let start = Instant::now();
        assert_eq!(spinner(start, start), "⠋");
        assert_eq!(spinner(start, start + SPIN), "⠙");
        assert_eq!(next_spin(start, start + SPIN / 2), start + SPIN);
    }
}
