//! Drawing the home screen.
//!
//! List rows are one line each with a blank line between, so the selection can
//! be a tinted band (a band spanning two lines would split in terminals with
//! line spacing). The detail pane leads with the instance and one clear action,
//! then tabs, then a glanceable overview.

use mctui_core::facets::{ModJar, ResourcePack};
use mctui_core::instance::Instance;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::layout::{self, Layout};
use super::{Facets, Filter, Home, Pane, Row, Tab, Target};
use crate::data::Data;
use crate::format;
use crate::theme::{self, Theme};
use crate::ui::{self, Canvas, text};

/// Each list row takes this many lines: the row and a blank line after it.
pub const ROW_PITCH: u16 = 2;

impl Home {
    pub fn render(&self, cv: &mut Canvas<'_>, data: &Data, splash: &str) {
        if data.instances.is_empty() {
            render_empty(cv, self.body, splash);
            return;
        }
        let lay = layout::layout(self.body);
        cv.clipped(lay.list, |cv| self.render_list(cv, &lay, data));
        match lay.detail {
            Some(area) => cv.clipped(area, |cv| self.render_detail(cv, area, data)),
            None => {
                if let (Some(area), Some(inst)) = (lay.summary, self.selected_instance(data)) {
                    let t = *cv.theme();
                    let line = text::fit(self.summary(&t, cv, inst), usize::from(area.width));
                    cv.line(area.x, area.y, &line, area.width);
                }
            }
        }
    }

    fn render_list(&self, cv: &mut Canvas<'_>, lay: &Layout, data: &Data) {
        let t = *cv.theme();
        let pane = lay.list;
        let focused = self.focus == Pane::List;
        cv.click(pane, ui::Target::Home(Target::List));

        let shown = self
            .rows
            .iter()
            .filter(|r| matches!(r, Row::Instance { .. }))
            .count();
        let total = data.instances.len();
        let count = if shown == total {
            total.to_string()
        } else {
            format!("{shown} of {total}")
        };
        let count = Line::styled(count, Style::new().fg(t.muted));
        match &self.filter {
            Filter::Editing(input) => {
                // The title row becomes a filled search field.
                cv.band(pane, t.surface);
                let inset = Rect {
                    width: pane.width.saturating_sub(2),
                    ..pane
                };
                let right = cv.line_right(inset, pane.y, &count);
                let x = pane.x + 2;
                cv.text(x, pane.y, "/", 1, Style::new().fg(t.accent).bold());
                let field = Rect::new(x + 2, pane.y, right.x.saturating_sub(x + 4), 1);
                input.render(
                    cv,
                    field,
                    Style::new().fg(t.strong),
                    "search instances",
                    true,
                );
            }
            Filter::Applied(input) => {
                let query = format!("“{}”  ", text::truncate(input.as_str(), 16));
                let right = Line::from(vec![
                    Span::styled(query, Style::new().fg(t.subtle)),
                    count.spans[0].clone(),
                ]);
                cv.zone(pane, "Instances", Some(right), focused);
            }
            Filter::Off => {
                cv.zone(pane, "Instances", Some(count), focused);
            }
        }

        let body = layout::list_rows(lay);
        if self.rows.is_empty() {
            cv.text(
                body.x + 2,
                body.y,
                "No matches",
                body.width,
                Style::new().fg(t.muted),
            );
            return;
        }
        let visible = self.visible_rows();
        let more = Style::new().fg(t.faint);
        if self.offset > 0 {
            let text = format!("↑ {} more", self.offset);
            cv.text(
                body.x + 2,
                body.y.saturating_sub(1),
                &text,
                body.width,
                more,
            );
        }
        for (i, row) in self.rows.iter().enumerate().skip(self.offset).take(visible) {
            let y = body.y + (i - self.offset) as u16 * ROW_PITCH;
            let area = Rect::new(body.x, y, body.width, 1);
            let target = ui::Target::Home(Target::Row(row.key()));
            let hovered = cv.hovered(&target);
            self.render_row(cv, area, row, focused && i == self.selected, hovered, data);
            cv.click(area, target);
        }
        let below = self.rows.len().saturating_sub(self.offset + visible);
        if below > 0 {
            cv.text(
                body.x + 2,
                body.bottom(),
                &format!("↓ {below} more"),
                body.width,
                more,
            );
        }
    }

    fn render_row(
        &self,
        cv: &mut Canvas<'_>,
        area: Rect,
        row: &Row,
        selected: bool,
        hovered: bool,
        data: &Data,
    ) {
        let t = *cv.theme();
        if selected {
            cv.band(area, t.selection);
        } else if hovered {
            cv.band(area, t.surface);
        }
        let inner = Rect {
            x: area.x + 2,
            width: area.width.saturating_sub(4),
            ..area
        };
        let name_style = if selected {
            Style::new().fg(t.strong).bold()
        } else if hovered {
            Style::new().fg(t.strong)
        } else {
            Style::new().fg(t.text)
        };

        let (index, positions) = match row {
            Row::New => {
                let style = if selected || hovered {
                    name_style
                } else {
                    Style::new().fg(t.muted)
                };
                cv.text(inner.x, inner.y, "+ New instance", inner.width, style);
                return;
            }
            Row::Instance {
                index, positions, ..
            } => (*index, positions),
        };
        let Some(inst) = data.instances.get(index) else {
            return;
        };

        // Right: the state that matters most — playing, new, or when played.
        let accessory = if self.playing.as_deref() == Some(inst.id.as_str()) {
            Line::styled(format!("{} Playing", ui::DOT), Style::new().fg(t.success))
        } else {
            match inst.last_played {
                None => Line::styled("new", Style::new().fg(t.secondary)),
                Some(ts) => {
                    let fg = if selected { t.subtle } else { t.dim };
                    Line::styled(
                        format::relative(ts, cv.cx.now, &cv.cx.tz),
                        Style::new().fg(fg),
                    )
                }
            }
        };
        let right = cv.line_right(inner, inner.y, &accessory);
        let room = right.x.saturating_sub(inner.x + 2);
        let hit = name_style.fg(t.accent);
        let name = Line::from(text::highlight(&inst.name, positions, name_style, hit));
        cv.line(inner.x, inner.y, &text::fit(name, usize::from(room)), room);
    }

    fn render_detail(&self, cv: &mut Canvas<'_>, area: Rect, data: &Data) {
        let t = *cv.theme();
        cv.click(area, ui::Target::Home(Target::Detail));
        let x = area.x + 2;
        let w = area.width.saturating_sub(2);
        let Some(inst) = self.selected_instance(data) else {
            // "+ New instance" is selected.
            cv.text(
                x,
                area.y,
                "New instance",
                w,
                Style::new().fg(t.strong).bold(),
            );
            let dim = Style::new().fg(t.dim);
            cv.text(
                x,
                area.y + 1,
                "Pick a Minecraft version, a loader, and a name.",
                w,
                dim,
            );
            let label = Line::from(vec![
                Span::styled("Create instance", Style::new().bold()),
                Span::raw("  ↵"),
            ]);
            button(cv, x, area.y + 3, &label, true, Target::Create);
            return;
        };
        let focused = self.focus == Pane::Detail;

        // Title, with its state chip flush right.
        let (status, status_bg) = self.status(&t, inst);
        let status_w = status.width() as u16 + 2;
        let chip = cv.pill(
            area.right().saturating_sub(status_w),
            area.y,
            &status,
            status_bg,
            status_w,
        );
        let title_fg = if focused { t.accent } else { t.strong };
        let room = chip.x.saturating_sub(x + 2);
        cv.text(
            x,
            area.y,
            &text::truncate(&inst.name, usize::from(room)),
            room,
            Style::new().fg(title_fg).bold(),
        );
        let identity = format!("Minecraft {}  ·  {}", inst.version, loader_label(inst));
        cv.text(
            x,
            area.y + 1,
            &text::truncate(&identity, usize::from(w)),
            w,
            Style::new().fg(t.dim),
        );

        self.actions(cv, x, area.y + 3, inst, data);
        self.tabs(cv, x, area.y + 5, w, inst);
        let top = area.y + 7;
        if top < area.bottom() {
            self.tab_body(cv, Rect::new(x, top, w, area.bottom() - top), inst);
        }
    }

    /// The instance's buttons: the main action filled, offline beside it.
    fn actions(&self, cv: &mut Canvas<'_>, x: u16, y: u16, inst: &Instance, data: &Data) {
        let playing = self.playing.as_deref() == Some(inst.id.as_str());
        let play = if playing {
            "View game"
        } else if data.accounts.active().is_none() {
            "Sign in & play"
        } else {
            "Play"
        };
        let label = Line::from(vec![
            Span::styled(format!("▶ {play}"), Style::new().bold()),
            Span::raw("   ↵"),
        ]);
        let end = button(cv, x, y, &label, true, Target::Play).right();
        if !playing {
            let label = Line::from(vec![Span::raw("Play offline"), Span::raw("   o")]);
            button(cv, end + 2, y, &label, false, Target::PlayOffline);
        }
    }

    /// Tab chips: "Overview", "Mods 3", "Resource packs 0". The active one is
    /// filled — with the accent tint while the pane has focus.
    fn tabs(&self, cv: &mut Canvas<'_>, x: u16, y: u16, w: u16, inst: &Instance) {
        let t = *cv.theme();
        let facets = self.facets.get(&inst.id);
        let mut cx = x;
        for tab in Tab::ALL {
            let room = (x + w).saturating_sub(cx);
            if room < 4 {
                break;
            }
            let target = ui::Target::Home(Target::Tab(tab));
            let active = tab == self.tab;
            let (fg, bg) = match (active, self.focus == Pane::Detail, cv.hovered(&target)) {
                (true, true, _) => (t.strong, t.selection),
                (true, false, _) => (t.strong, t.surface),
                (false, _, true) => (t.text, t.surface),
                (false, _, false) => (t.dim, Color::Reset),
            };
            let label = if active {
                Style::new().fg(fg).bold()
            } else {
                Style::new().fg(fg)
            };
            let mut spans = vec![Span::styled(tab.title(), label)];
            let count = match tab {
                Tab::Overview => None,
                Tab::Mods => Some(facets.map_or(0, |f| f.mods.len())),
                Tab::ResourcePacks => Some(facets.map_or(0, |f| f.packs.len())),
            };
            if let Some(n) = count {
                let fg = if active { t.subtle } else { t.muted };
                spans.push(Span::styled(format!(" {n}"), Style::new().fg(fg)));
            }
            let chip = cv.pill(cx, y, &Line::from(spans), bg, room);
            cv.click(chip, target);
            cx = chip.right() + 1;
        }
    }

    fn tab_body(&self, cv: &mut Canvas<'_>, area: Rect, inst: &Instance) {
        let facets = self.facets.get(&inst.id);
        match self.tab {
            Tab::Overview => overview(cv, area, inst, facets),
            Tab::Mods => {
                let mods = facets.map(|f| &f.mods[..]).unwrap_or_default();
                let empty = if inst.loader.supports_mod_browser() {
                    "No mods yet. Press ↵ to browse Modrinth."
                } else {
                    "Mods need a Fabric instance."
                };
                preview(cv, area, mods, mod_row, empty);
            }
            Tab::ResourcePacks => {
                let packs = facets.map(|f| &f.packs[..]).unwrap_or_default();
                let empty = "No resource packs yet. Press ↵ to add some.";
                preview(cv, area, packs, pack_row, empty);
            }
        }
    }

    /// The title's state chip: playing, ready, or not downloaded yet.
    fn status(&self, t: &Theme, inst: &Instance) -> (Line<'static>, Color) {
        let (text, fg, bg) = if self.playing.as_deref() == Some(inst.id.as_str()) {
            (format!("{} Playing", ui::DOT), t.success, t.success_bg)
        } else if inst.is_fully_downloaded {
            (format!("{} Ready", ui::DOT), t.success, t.success_bg)
        } else {
            ("↓ Not downloaded".to_owned(), t.dim, t.surface)
        };
        (Line::from(Span::styled(text, Style::new().fg(fg))), bg)
    }

    /// Compact mode's one-liner: "● Ready · 1.21.4 · Fabric 0.16.10 · 12 mods · 2h ago".
    fn summary(&self, t: &Theme, cv: &Canvas<'_>, inst: &Instance) -> Line<'static> {
        let sep = || Span::styled(" · ", Style::new().fg(t.muted));
        let mods = self.facets.get(&inst.id).map_or(0, |f| f.mods.len());
        let played = inst.last_played.map_or_else(
            || "Never played".to_owned(),
            |ts| format::relative(ts, cv.cx.now, &cv.cx.tz),
        );
        let mut spans = self.status(t, inst).0.spans;
        spans.extend([
            sep(),
            Span::styled(inst.version.clone(), Style::new().fg(t.text)),
            sep(),
            Span::styled(loader_label(inst), Style::new().fg(t.dim)),
            sep(),
            Span::styled(format::count(mods, "mod"), Style::new().fg(t.dim)),
            sep(),
            Span::styled(played, Style::new().fg(t.dim)),
        ]);
        Line::from(spans)
    }
}

/// A clickable button: `primary` ones are filled with the accent, others sit on
/// a quiet surface. Brightens under the pointer. Returns its area.
fn button(
    cv: &mut Canvas<'_>,
    x: u16,
    y: u16,
    label: &Line<'_>,
    primary: bool,
    target: Target,
) -> Rect {
    let t = *cv.theme();
    let target = ui::Target::Home(target);
    let hovered = cv.hovered(&target);
    let (bg, fg) = match (primary, hovered) {
        (true, false) => (t.accent, theme::on_color(t.accent)),
        (true, true) => (t.accent_hi, theme::on_color(t.accent_hi)),
        (false, false) => (t.surface, t.text),
        (false, true) => (t.surface_hi, t.strong),
    };
    // A cell wider than a chip on each side: buttons should look pressable.
    let mut spans = vec![Span::raw(" ")];
    spans.extend(label.spans.iter().cloned());
    spans.push(Span::raw(" "));
    let line = Line::from(spans).patch_style(Style::new().fg(fg));
    let area = cv.pill(x, y, &line, bg, 48);
    cv.click(area, target);
    area
}

/// The overview: a row of stat tiles, then recent worlds.
fn overview(cv: &mut Canvas<'_>, area: Rect, inst: &Instance, facets: Option<&Facets>) {
    const MIN_TILE: u16 = 13;
    let t = *cv.theme();
    let (now, tz) = (cv.cx.now, cv.cx.tz.clone());
    let worlds = facets.map_or(&[][..], |f| &f.worlds[..]);
    let last = inst
        .last_played
        .map_or_else(|| "Never".to_owned(), |ts| format::relative(ts, now, &tz));
    let mut stats = vec![
        (format::play_time(inst.play_time_secs), "played"),
        (last, "last played"),
        (
            worlds.len().to_string(),
            if worlds.len() == 1 { "world" } else { "worlds" },
        ),
    ];
    if inst.loader.supports_mod_browser() {
        let mods = facets.map_or(0, |f| f.mods.len());
        stats.push((mods.to_string(), if mods == 1 { "mod" } else { "mods" }));
    }

    // Tiles share the width, and wrap into rows when the pane is narrow.
    let per_row = (area.width / MIN_TILE).clamp(1, stats.len() as u16);
    let tile_w = area.width / per_row;
    let mut y = area.y;
    for chunk in stats.chunks(usize::from(per_row)) {
        if y + 1 >= area.bottom() {
            return;
        }
        for (i, (value, label)) in chunk.iter().enumerate() {
            let tx = area.x + i as u16 * tile_w;
            let w = tile_w.saturating_sub(1);
            cv.text(tx, y, value, w, Style::new().fg(t.strong).bold());
            cv.text(tx, y + 1, label, w, Style::new().fg(t.muted));
        }
        y += 3;
    }

    if worlds.is_empty() || y + 1 >= area.bottom() {
        return;
    }
    cv.text(
        area.x,
        y,
        "Worlds",
        area.width,
        Style::new().fg(t.subtle).bold(),
    );
    y += 1;
    let room = usize::from(area.bottom().saturating_sub(y));
    let (shown, rest) = if worlds.len() > room {
        (
            room.saturating_sub(1),
            worlds.len() - room.saturating_sub(1),
        )
    } else {
        (worlds.len(), 0)
    };
    for world in &worlds[..shown] {
        let row = Rect::new(area.x, y, area.width, 1);
        let when = world
            .modified
            .map(|ts| format::relative(ts, now, &tz))
            .unwrap_or_default();
        let right = cv.line_right(row, y, &Line::styled(when, Style::new().fg(t.muted)));
        let room = right.x.saturating_sub(row.x + 2);
        cv.text(
            row.x,
            y,
            &text::truncate(&world.name, usize::from(room)),
            room,
            Style::new().fg(t.text),
        );
        y += 1;
    }
    if rest > 0 {
        cv.text(
            area.x,
            y,
            &format!("…and {rest} more"),
            area.width,
            Style::new().fg(t.faint),
        );
    }
}

/// "Fabric 0.16.10", or just "Vanilla".
fn loader_label(inst: &Instance) -> String {
    let name = inst.loader.display_name();
    if inst.loader_version.is_empty() || name == "Vanilla" {
        name.to_owned()
    } else {
        format!("{name} {}", inst.loader_version)
    }
}

/// A read-only preview of a manager screen: up to the rows that fit, then
/// "…and N more". Clicking it opens the manager. Only the rows shown are
/// formatted.
fn preview<T>(
    cv: &mut Canvas<'_>,
    area: Rect,
    items: &[T],
    row: impl Fn(&T) -> (&str, String),
    empty: &str,
) {
    let t = *cv.theme();
    cv.click(area, ui::Target::Home(Target::Preview));
    if items.is_empty() {
        cv.text(area.x, area.y, empty, area.width, Style::new().fg(t.faint));
        return;
    }
    let fit = usize::from(area.height.max(1));
    let (shown, rest) = if items.len() > fit {
        (fit - 1, items.len() - (fit - 1))
    } else {
        (items.len(), 0)
    };
    for (i, item) in items.iter().take(shown).enumerate() {
        let (label, right) = row(item);
        let y = area.y + i as u16;
        let line = Rect::new(area.x, y, area.width, 1);
        let right = cv.line_right(line, y, &Line::styled(right, Style::new().fg(t.muted)));
        let room = right.x.saturating_sub(line.x + 2);
        cv.text(
            line.x,
            y,
            &text::truncate(label, usize::from(room)),
            room,
            Style::new().fg(t.text),
        );
    }
    if rest > 0 {
        let y = area.y + shown as u16;
        cv.text(
            area.x,
            y,
            &format!("…and {rest} more"),
            area.width,
            Style::new().fg(t.faint),
        );
    }
}

fn mod_row(m: &ModJar) -> (&str, String) {
    (&m.display_name, format::size(m.size))
}

/// A pack's category (from Vanilla Tweaks) or its file size.
fn pack_row(p: &ResourcePack) -> (&str, String) {
    let right = p.category.clone().or_else(|| p.size.map(format::size));
    (&p.name, right.unwrap_or_default())
}

/// No instances yet: the wordmark, the splash, and one thing to do.
fn render_empty(cv: &mut Canvas<'_>, body: Rect, splash: &str) {
    let t = *cv.theme();
    let lines: [(&str, Style); 3] = [
        ("mctui", Style::new().fg(t.accent).bold()),
        (
            "A Minecraft launcher for your terminal",
            Style::new().fg(t.dim),
        ),
        (splash, Style::new().fg(t.warning_soft).italic()),
    ];
    let top = body.y + body.height.saturating_sub(6) / 2;
    let center = |w: usize| body.x + body.width.saturating_sub(w as u16) / 2;
    for (i, (s, style)) in lines.iter().enumerate() {
        cv.text(
            center(text::width(s)),
            top + i as u16,
            s,
            body.width,
            *style,
        );
    }
    let label = Line::from(vec![
        Span::styled("Create your first instance", Style::new().bold()),
        Span::raw("   ↵"),
    ]);
    // The button pads its label by 2 cells each side.
    button(
        cv,
        center(label.width() + 4),
        top + 5,
        &label,
        true,
        Target::Create,
    );
}
