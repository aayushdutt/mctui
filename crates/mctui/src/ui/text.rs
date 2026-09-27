//! Width-aware text helpers. Widths are terminal cells, not bytes or chars.

use std::borrow::Cow;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const ELLIPSIS: &str = "…";

/// Display width of `s` in terminal cells.
pub fn width(s: &str) -> usize {
    s.width()
}

/// Cuts `s` to at most `max` cells, ending in `…` when anything was dropped.
pub fn truncate(s: &str, max: usize) -> Cow<'_, str> {
    if s.width() <= max {
        return Cow::Borrowed(s);
    }
    if max == 0 {
        return Cow::Borrowed("");
    }
    let mut out = String::new();
    let mut used = 0;
    for g in s.graphemes(true) {
        let w = g.width();
        if used + w > max - 1 {
            break;
        }
        out.push_str(g);
        used += w;
    }
    out.push_str(ELLIPSIS);
    Cow::Owned(out)
}

/// Cuts a styled line to at most `max` cells, ending in `…` (styled like the
/// span it replaces) when anything was dropped.
pub fn fit(line: Line<'_>, max: usize) -> Line<'_> {
    if line.width() <= max {
        return line;
    }
    let mut spans: Vec<Span<'_>> = Vec::with_capacity(line.spans.len());
    let mut room = max;
    for span in line.spans {
        let w = span.width();
        if w < room {
            room -= w;
            spans.push(span);
            continue;
        }
        let cut = truncate(&span.content, room).into_owned();
        spans.push(Span::styled(cut, span.style));
        break;
    }
    Line::from(spans).style(line.style)
}

/// Splits `s` into lines of at most `max` cells, breaking at spaces where it
/// can and inside long words where it must.
pub fn wrap(s: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let sep = usize::from(!cur.is_empty());
            if width(&cur) + sep + width(word) <= max {
                if sep == 1 {
                    cur.push(' ');
                }
                cur.push_str(word);
                continue;
            }
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            // A word longer than the line: hard-break it.
            for g in word.graphemes(true) {
                if width(&cur) + g.width() > max {
                    lines.push(std::mem::take(&mut cur));
                }
                cur.push_str(g);
            }
        }
        lines.push(cur);
    }
    lines
}

/// Splits `label` into spans with the matched graphemes (the fuzzy matcher
/// counts in graphemes) in `hit` style.
pub fn highlight<'a>(label: &'a str, positions: &[u32], base: Style, hit: Style) -> Vec<Span<'a>> {
    if positions.is_empty() {
        return vec![Span::styled(label, base)];
    }
    let mut spans = Vec::new();
    let mut run_start = 0;
    let mut run_hit = false;
    for (i, (byte, _)) in label.grapheme_indices(true).enumerate() {
        let is_hit = positions.binary_search(&(i as u32)).is_ok();
        if i == 0 {
            run_hit = is_hit;
        } else if is_hit != run_hit {
            spans.push(Span::styled(
                &label[run_start..byte],
                if run_hit { hit } else { base },
            ));
            run_start = byte;
            run_hit = is_hit;
        }
    }
    spans.push(Span::styled(
        &label[run_start..],
        if run_hit { hit } else { base },
    ));
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_text_and_marks_cuts() {
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello", 4), "hel…");
        assert_eq!(truncate("hello", 1), "…");
        assert_eq!(truncate("hello", 0), "");
        // Wide glyphs never split: 2 cells each.
        assert_eq!(truncate("日本語", 4), "日…");
    }

    #[test]
    fn fit_cuts_across_spans() {
        let line = Line::from(vec![Span::raw("abc"), Span::raw("defg")]);
        assert_eq!(fit(line, 5).to_string(), "abcd…");
    }

    #[test]
    fn highlight_splits_runs() {
        let spans = highlight("Mods", &[0, 1], Style::new(), Style::new().bold());
        let parts: Vec<&str> = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(parts, ["Mo", "ds"]);
    }

    #[test]
    fn wrap_breaks_at_spaces_and_inside_long_words() {
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(wrap("a\nb", 10), ["a", "b"]);
    }
}
