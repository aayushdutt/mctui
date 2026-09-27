//! Drawing the mods screen.
//!
//! Every row is one line; the only tints are the selection and hover bands,
//! the search field and the buttons, each a single row.

use mctui_core::modrinth::{SearchHit, Side};
use mctui_core::net::Progress;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::{Focus, Mods, Pane, Search, Target, layout};
use crate::format;
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{self, Canvas, text};

/// At most this many lines of warnings show after an install.
const MAX_NOTE_LINES: usize = 4;
/// Rows the install's progress takes: what's happening, then the bar.
const INSTALL_ROWS: u16 = 2;
/// Between the parts of a meta line.
const SEP: &str = "  ·  ";

/// Categories that only name a loader: every result here is a Fabric mod.
const LOADERS: &[&str] = &[
    "fabric",
    "quilt",
    "forge",
    "neoforge",
    "babric",
    "bta-babric",
    "legacy-fabric",
    "ornithe",
    "rift",
    "liteloader",
    "modloader",
    "nilloader",
    "java-agent",
];

impl Mods {
    pub(super) fn draw(&self, cv: &mut Canvas<'_>) {
        let lay = layout::layout(self.body, self.focus.pane());
        let compact = layout::compact(self.body);
        if let Some(pane) = lay.installed {
            cv.clipped(pane, |cv| self.draw_installed(cv, pane, compact));
        }
        if let Some(pane) = lay.discover {
            let below = lay.details.is_none();
            cv.clipped(pane, |cv| self.draw_discover(cv, pane, compact, below));
        }
        if let Some(area) = lay.details {
            cv.clipped(area, |cv| self.draw_column(cv, area));
        }
    }

    /// Rows above the results, in a Discover pane `width` wide, for the
    /// running install or the last one's warnings.
    pub(super) fn activity_rows(&self, width: u16) -> u16 {
        if self.install.is_some() {
            INSTALL_ROWS
        } else {
            self.note_lines(width).len() as u16
        }
    }

    /// The last install's warnings, wrapped to fit a Discover pane `width`
    /// wide: each after a ▲, and its other lines under its first.
    fn note_lines(&self, width: u16) -> Vec<String> {
        let room = usize::from(width.saturating_sub(4 + 3));
        self.notes
            .iter()
            .flat_map(|note| {
                text::wrap(note, room)
                    .into_iter()
                    .enumerate()
                    .map(|(i, line)| match i {
                        0 => format!("{}  {line}", ui::WARN),
                        _ => format!("   {line}"),
                    })
            })
            .take(MAX_NOTE_LINES)
            .collect()
    }

    fn draw_installed(&self, cv: &mut Canvas<'_>, pane: Rect, compact: bool) {
        let t = *cv.theme();
        let focused = self.focus == Focus::Installed;
        cv.click(pane, ui::Target::Mods(Target::Pane(Pane::Installed)));
        let parts = layout::installed(pane);

        let label = inset(parts.label);
        let count = self.installed.mods.as_ref().map(|m| m.len().to_string());
        widgets::section(cv, label, "Installed", count.as_deref(), focused);
        if compact {
            switch(cv, label, Pane::Discover, "Discover →");
        }
        // What mods here must be for; the loader goes first when it's tight.
        let about = [
            format!("Minecraft {}", self.inst.version),
            self.inst.loader.display_name().to_owned(),
        ];
        let about = pack(&about, usize::from(parts.info.width.saturating_sub(4)));
        lines(
            cv,
            inset(parts.info),
            &about[..about.len().min(1)],
            Style::new().fg(t.dim),
        );

        let Some(mods) = &self.installed.mods else {
            return;
        };
        if mods.is_empty() {
            let empty = if compact {
                "No mods yet — press tab to search Modrinth."
            } else {
                "No mods yet — search Modrinth on the right."
            };
            let area = inset(parts.list);
            let copy = text::wrap(empty, usize::from(area.width));
            lines(cv, area, &copy, Style::new().fg(t.faint));
            return;
        }
        self.installed
            .list
            .render(cv, parts.list, focused, |cv, row, i, selected, hovered| {
                let Some(m) = mods.get(i) else {
                    return;
                };
                let right = m.version.clone().unwrap_or_else(|| format::size(m.size));
                let right = text::truncate(&right, 14).into_owned();
                let fg = if selected { t.subtle } else { t.dim };
                let right = cv.line_right(row, row.y, &Line::styled(right, Style::new().fg(fg)));
                let room = right.x.saturating_sub(row.x + 2);
                let title = text::truncate(&m.title, usize::from(room));
                let style = widgets::row_label(&t, selected, hovered);
                cv.text(row.x, row.y, &title, room, style);
            });
    }

    fn draw_discover(&self, cv: &mut Canvas<'_>, pane: Rect, compact: bool, details: bool) {
        let t = *cv.theme();
        let focused = self.focus != Focus::Installed;
        cv.click(pane, ui::Target::Mods(Target::Pane(Pane::Discover)));
        let parts = layout::discover(pane, self.activity_rows(pane.width), details);

        let label = inset(parts.label);
        let d = &self.discover;
        if let Search::Loading {
            more: false,
            started,
            ..
        } = &d.search
        {
            widgets::section(cv, label, "Discover", None, focused);
            let x = label.x + text::width("DISCOVER") as u16 + 3;
            let spin = widgets::spinner(*started, self.now);
            cv.text(x, label.y, spin, 1, Style::new().fg(t.accent));
        } else {
            let count = (!d.hits.is_empty()).then(|| {
                if d.hits.len() < d.total {
                    format!("{} of {}", d.hits.len(), thousands(d.total))
                } else {
                    thousands(d.total)
                }
            });
            widgets::section(cv, label, "Discover", count.as_deref(), focused);
        }
        if compact {
            let n = self.installed.mods.as_ref().map_or(0, Vec::len);
            switch(cv, label, Pane::Installed, &format!("← Installed {n}"));
        }

        self.draw_field(cv, parts.field);
        self.draw_activity(cv, parts.activity, pane.width);
        let used = self.draw_results(cv, parts.list);
        if let Some(area) = parts.details {
            // Right under a short list; at the bottom under a full one.
            let y = area.y.min(parts.list.y + used + 1);
            self.draw_block(cv, Rect { y, ..area });
        }
    }

    /// The search field: a filled row with `/` (its key) and the query.
    fn draw_field(&self, cv: &mut Canvas<'_>, row: Rect) {
        let t = *cv.theme();
        let focused = self.focus == Focus::Search;
        let target = ui::Target::Mods(Target::Search);
        let bg = match (focused, cv.hovered(&target)) {
            (true, _) => t.selection,
            (false, true) => t.surface_hi,
            (false, false) => t.surface,
        };
        cv.band(row, bg);
        let slash = if focused {
            Style::new().fg(t.accent).bold()
        } else {
            Style::new().fg(t.muted)
        };
        cv.text(row.x + 2, row.y, "/", 1, slash);
        let input = Rect::new(row.x + 4, row.y, row.width.saturating_sub(6), 1);
        let style = Style::new().fg(if focused { t.strong } else { t.text });
        self.discover
            .input
            .render(cv, input, style, "Search Modrinth", focused);
        cv.click(row, target);
    }

    /// The running install (what it's doing, then its progress bar), or the
    /// last one's warnings.
    fn draw_activity(&self, cv: &mut Canvas<'_>, area: Rect, pane_width: u16) {
        let t = *cv.theme();
        let area = inset(area);
        if area.is_empty() {
            return;
        }
        let Some(install) = &self.install else {
            let notes = self.note_lines(pane_width);
            lines(cv, area, &notes, Style::new().fg(t.warning));
            return;
        };
        let spin = widgets::spinner(install.started, self.now);
        let right = install
            .progress
            .map_or(Rect::new(area.right(), area.y, 0, 1), |p| {
                let files = format!(
                    "{} of {}",
                    p.files_done,
                    format::count(p.files_total, "file")
                );
                let right = if p.bytes_total == 0 {
                    files
                } else {
                    format!("{}{SEP}{files}", amount(p.bytes_done, p.bytes_total))
                };
                cv.line_right(area, area.y, &Line::styled(right, Style::new().fg(t.muted)))
            });
        let room = right.x.saturating_sub(area.x + 2);
        let what = Line::from(vec![
            Span::styled(spin, Style::new().fg(t.accent)),
            Span::styled("  Installing ", Style::new().fg(t.dim)),
            Span::styled(install.title.as_str(), Style::new().fg(t.strong).bold()),
        ]);
        cv.line(area.x, area.y, &text::fit(what, usize::from(room)), room);
        let bar = Rect {
            y: area.y + 1,
            ..area
        };
        if let Some(p) = install.progress {
            widgets::progress(cv, bar, ratio(p));
        } else {
            let muted = Style::new().fg(t.muted);
            cv.text(bar.x, bar.y, "Checking dependencies…", bar.width, muted);
        }
    }

    /// The results, or why there are none. Returns the rows used.
    fn draw_results(&self, cv: &mut Canvas<'_>, area: Rect) -> u16 {
        let t = *cv.theme();
        let d = &self.discover;
        if d.hits.is_empty() {
            let version = &self.inst.version;
            let (copy, fg) = match &d.search {
                Search::Loading { .. } if d.query.is_empty() => (
                    format!("Finding popular mods for Minecraft {version}…"),
                    t.faint,
                ),
                Search::Loading { .. } => ("Searching Modrinth…".to_owned(), t.faint),
                Search::Failed { message, .. } => (message.clone(), t.dim),
                Search::Idle if d.query.is_empty() => (
                    format!("Modrinth has no Fabric mods for Minecraft {version} yet."),
                    t.faint,
                ),
                Search::Idle => (
                    format!(
                        "Nothing matches “{}” for Minecraft {version}.\n\
                         Try other words, or clear the search to see popular mods.",
                        d.query
                    ),
                    t.faint,
                ),
            };
            let area = inset(area);
            let copy = text::wrap(&copy, usize::from(area.width));
            return lines(cv, area, &copy, Style::new().fg(fg));
        }

        let focused = self.focus == Focus::Results;
        d.list
            .render(cv, area, focused, |cv, row, i, selected, hovered| {
                if let Some(hit) = d.hits.get(i) {
                    self.draw_hit(cv, row, hit, selected, hovered);
                }
            });

        // Rows sit two lines apart; the list keeps its last line for "↓ N more".
        let fits = usize::from(area.height.saturating_sub(1).div_ceil(2)).max(1);
        let full = d.hits.len() > fits;
        let tail_y = if full {
            area.bottom().saturating_sub(1)
        } else {
            area.y + d.hits.len() as u16 * 2
        };
        // The next page, loading or failed, where the list ends.
        let tail = match &d.search {
            Search::Loading {
                more: true,
                started,
                ..
            } => Line::from(vec![
                Span::styled(
                    widgets::spinner(*started, self.now),
                    Style::new().fg(t.accent),
                ),
                Span::styled("  Loading more…", Style::new().fg(t.muted)),
            ]),
            Search::Failed {
                more: true,
                message,
            } => Line::styled(message.as_str(), Style::new().fg(t.dim)),
            _ if full => return area.height,
            _ => return (d.hits.len() * 2 - 1) as u16,
        };
        let line = inset(Rect {
            y: tail_y,
            height: 1,
            ..area
        });
        cv.clear(line.intersection(area));
        let tail = text::fit(tail, usize::from(line.width));
        cv.line(line.x, line.y, &tail, line.width);
        if full {
            area.height
        } else {
            tail_y + 1 - area.y
        }
    }

    /// One result: its title, and whether it's installed or how popular.
    fn draw_hit(
        &self,
        cv: &mut Canvas<'_>,
        row: Rect,
        hit: &SearchHit,
        selected: bool,
        hovered: bool,
    ) {
        let t = *cv.theme();
        let accessory = match &self.install {
            Some(install) if install.project_id == hit.project_id => Line::from(vec![
                Span::styled(
                    widgets::spinner(install.started, self.now),
                    Style::new().fg(t.accent),
                ),
                Span::styled(" Installing", Style::new().fg(t.dim)),
            ]),
            _ if self.installed_jar(hit).is_some() => Line::styled(
                format!("{} Installed", ui::CHECK),
                Style::new().fg(t.success),
            ),
            _ => {
                let fg = if selected { t.subtle } else { t.dim };
                Line::styled(downloads(hit.downloads), Style::new().fg(fg))
            }
        };
        let right = cv.line_right(row, row.y, &accessory);
        let room = right.x.saturating_sub(row.x + 2);
        let title = text::truncate(&hit.title, usize::from(room));
        let style = widgets::row_label(&t, selected, hovered);
        cv.text(row.x, row.y, &title, room, style);
    }

    /// The selected result under the results: title, one line of who and
    /// what, two of its summary, one of stats, then the buttons. Brief (when
    /// `area` is short of the full block) keeps one line of summary and drops
    /// the stats.
    fn draw_block(&self, cv: &mut Canvas<'_>, area: Rect) {
        let Some(hit) = self.selected_hit() else {
            return;
        };
        let t = *cv.theme();
        let brief = area.height < layout::DETAILS_FULL - 1;
        let area = inset(area);
        let w = usize::from(area.width);
        let row = |dy: u16, height: u16| Rect::new(area.x, area.y + dy, area.width, height);

        title(cv, row(0, 1), &hit.title);
        let (author, tags) = meta(hit);
        let meta: Vec<String> = author.into_iter().chain(tags).collect();
        let meta = pack(&meta, w);
        lines(
            cv,
            row(1, 1),
            &meta[..meta.len().min(1)],
            Style::new().fg(t.dim),
        );
        let summary = text::wrap(&hit.description, w);
        let summary_rows = if brief { 1 } else { 2 };
        lines(cv, row(2, summary_rows), &summary, Style::new().fg(t.text));
        if !brief {
            let stats = pack(&stats(hit, cv), w);
            lines(
                cv,
                row(4, 1),
                &stats[..stats.len().min(1)],
                Style::new().fg(t.muted),
            );
        }
        let buttons = if brief { 4 } else { 6 };
        self.buttons(cv, row(buttons, 1), hit);
    }

    /// The selected result in a column of its own: the same, with room to
    /// breathe.
    fn draw_column(&self, cv: &mut Canvas<'_>, area: Rect) {
        let Some(hit) = self.selected_hit() else {
            return;
        };
        let t = *cv.theme();
        let area = inset(area);
        let w = usize::from(area.width);
        let rows = |y: u16, height: u16| Rect::new(area.x, y, area.width, height);
        let (author, tags) = meta(hit);
        let dim = Style::new().fg(t.dim);

        title(cv, rows(area.y, 1), &hit.title);
        let mut y = area.y + 1;
        y += lines(cv, rows(y, 1), author.as_slice(), dim);
        y += lines(cv, rows(y, 2), &pack(&tags, w), dim) + 1;
        // Leave room for a blank, two lines of stats, a blank and the buttons.
        let room = area.bottom().saturating_sub(y + 7);
        let summary = text::wrap(&hit.description, w);
        y += lines(cv, rows(y, room), &summary, Style::new().fg(t.text)) + 1;
        let muted = Style::new().fg(t.muted);
        y += lines(cv, rows(y, 2), &pack(&stats(hit, cv), w), muted) + 1;
        self.buttons(cv, rows(y, 3), hit);
    }

    /// Install (or Update) and the Modrinth page: side by side if they fit,
    /// else one over the other if `area` has the rows, else just the first.
    fn buttons(&self, cv: &mut Canvas<'_>, area: Rect, hit: &SearchHit) {
        let (label, key, kind) = if self.installing(hit) {
            ("Installing…", None, ButtonKind::Secondary)
        } else if self.installed_jar(hit).is_some() {
            ("Update", Some("↵"), ButtonKind::Primary)
        } else {
            ("Install", Some("↵"), ButtonKind::Primary)
        };
        let install = ui::Target::Mods(Target::Install);
        let end = widgets::button(cv, area.x, area.y, label, key, kind, install).right();
        let (page, page_key) = ("Modrinth page", Some("o"));
        let (x, y) = if end + 2 + button_width(page, page_key) <= area.right() {
            (end + 2, area.y)
        } else if area.height >= 3 {
            (area.x, area.y + 2)
        } else {
            return;
        };
        let target = ui::Target::Mods(Target::Page);
        widgets::button(cv, x, y, page, page_key, ButtonKind::Secondary, target);
    }
}

/// A result's title, bold.
fn title(cv: &mut Canvas<'_>, row: Rect, title: &str) {
    let strong = cv.theme().strong;
    let title = text::truncate(title, usize::from(row.width));
    cv.text(
        row.x,
        row.y,
        &title,
        row.width,
        Style::new().fg(strong).bold(),
    );
}

/// The compact layout's way to the other pane, flush right on the label row.
fn switch(cv: &mut Canvas<'_>, row: Rect, to: Pane, label: &str) {
    let t = *cv.theme();
    let target = ui::Target::Mods(Target::Pane(to));
    let fg = if cv.hovered(&target) { t.text } else { t.muted };
    let area = cv.line_right(row, row.y, &Line::styled(label, Style::new().fg(fg)));
    cv.click(area, target);
}

/// `area` less the 2 columns text keeps from a pane's edges.
fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x + 2.min(area.width),
        width: area.width.saturating_sub(4),
        ..area
    }
}

/// Draws as many of `copy` as fit in `area`, the last marked `…` if some
/// don't. Returns the rows used.
fn lines(cv: &mut Canvas<'_>, area: Rect, copy: &[String], style: Style) -> u16 {
    let room = usize::from(area.height);
    for (i, line) in copy.iter().take(room).enumerate() {
        let cut = i + 1 == room && copy.len() > room;
        let line = if cut {
            format!("{line}{}", text::ELLIPSIS)
        } else {
            line.clone()
        };
        let line = text::truncate(&line, usize::from(area.width));
        cv.text(area.x, area.y + i as u16, &line, area.width, style);
    }
    copy.len().min(room) as u16
}

/// Joins `parts` with `SEP` into lines of at most `width`, never splitting a
/// part (one too long for a line of its own is cut).
fn pack(parts: &[String], width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in parts {
        match out.last_mut() {
            Some(line) if text::width(line) + text::width(SEP) + text::width(part) <= width => {
                line.push_str(SEP);
                line.push_str(part);
            }
            _ => out.push(text::truncate(part, width).into_owned()),
        }
    }
    out
}

/// Who made it ("by jellysquid3") and what it is ("Optimization",
/// "Client-side").
fn meta(hit: &SearchHit) -> (Option<String>, Vec<String>) {
    let author = (!hit.author.is_empty()).then(|| format!("by {}", hit.author));
    let categories = hit
        .display_categories
        .iter()
        .filter(|c| !LOADERS.contains(&c.as_str()))
        .take(3)
        .map(|c| category(c));
    let side = side(hit.client_side, hit.server_side).map(str::to_owned);
    (author, categories.chain(side).collect())
}

/// How popular and fresh it is: "231.7M downloads", "Updated 6d ago".
fn stats(hit: &SearchHit, cv: &Canvas<'_>) -> Vec<String> {
    let updated = format::relative(hit.date_modified, cv.cx.now, &cv.cx.tz)
        .replacen("Just now", "just now", 1);
    vec![
        format!("{} downloads", downloads(hit.downloads)),
        format!("Updated {updated}"),
    ]
}

/// "game-mechanics" → "Game mechanics".
fn category(slug: &str) -> String {
    let words = slug.replace('-', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Where a mod runs, when that's something to know.
fn side(client: Side, server: Side) -> Option<&'static str> {
    let runs = |side| matches!(side, Side::Required | Side::Optional);
    match (runs(client), runs(server)) {
        (true, false) if server == Side::Unsupported => Some("Client-side"),
        (false, true) if client == Side::Unsupported => Some("Server-side"),
        (true, true) => Some("Client and server"),
        _ => None,
    }
}

/// The cells [`widgets::button`] takes for `label` and `key`: two of
/// padding each side, and two before the key.
fn button_width(label: &str, key: Option<&str>) -> u16 {
    let key = key.map_or(0, |k| 2 + text::width(k));
    (text::width(label) + key + 4) as u16
}

/// "231.7M", "12.3K", "999".
fn downloads(n: u64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "one decimal of a download count"
    )]
    let per = |unit: u64| n as f64 / unit as f64;
    match n {
        1_000_000_000.. => format!("{:.1}B", per(1_000_000_000)),
        1_000_000.. => format!("{:.1}M", per(1_000_000)),
        1_000.. => format!("{:.1}K", per(1_000)),
        _ => n.to_string(),
    }
}

/// "5,056".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "1.2 of 3.4 MB", in the total's unit.
fn amount(done: u64, total: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    #[expect(clippy::cast_precision_loss, reason = "one decimal of a size")]
    let per = |bytes: u64, unit: u64| bytes as f64 / unit as f64;
    if total >= MIB {
        format!("{:.1} of {:.1} MB", per(done, MIB), per(total, MIB))
    } else {
        format!("{} of {} KB", done / KIB, total.div_ceil(KIB))
    }
}

/// How far along a download is: by bytes when their total is known.
fn ratio(p: Progress) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "a ratio for a progress bar")]
    let ratio = |done: u64, total: u64| done as f64 / total as f64;
    match (p.bytes_total, p.files_total) {
        (0, 0) => 0.0,
        (0, files) => ratio(p.files_done as u64, files as u64),
        (bytes, _) => ratio(p.bytes_done, bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_compact() {
        assert_eq!(downloads(999), "999");
        assert_eq!(downloads(12_345), "12.3K");
        assert_eq!(downloads(146_600_000), "146.6M");
        assert_eq!(downloads(1_200_000_000), "1.2B");
        assert_eq!(thousands(5056), "5,056");
        assert_eq!(thousands(34), "34");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(amount(310_000, 3_500_000), "0.3 of 3.3 MB");
        assert_eq!(amount(2048, 10_000), "2 of 10 KB");
    }

    #[test]
    fn meta_reads_like_words_and_packs_whole_parts() {
        assert_eq!(category("game-mechanics"), "Game mechanics");
        assert_eq!(side(Side::Required, Side::Unsupported), Some("Client-side"));
        assert_eq!(side(Side::Unsupported, Side::Optional), Some("Server-side"));
        assert_eq!(
            side(Side::Optional, Side::Required),
            Some("Client and server")
        );
        assert_eq!(side(Side::Unknown, Side::Unknown), None);
        let parts = ["by Prospector", "Library", "Utility", "Client-side"].map(String::from);
        assert_eq!(
            pack(&parts, 30),
            ["by Prospector  ·  Library", "Utility  ·  Client-side"]
        );
        assert_eq!(pack(&parts[..1], 5), ["by P…"]);
    }
}
