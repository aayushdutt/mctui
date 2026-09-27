//! Home screen geometry: pure functions of the body area.

use ratatui::layout::Rect;

/// At or above this body width home shows the list and detail side by side;
/// below it, one full-width list and a summary line. 76 is the body width of
/// an 80-column terminal, so the common 80×24 gets both columns.
pub const SPLIT_MIN_WIDTH: u16 = 76;

const LIST_MIN: u16 = 30;
const LIST_MAX: u16 = 46;
const GAP: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub list: Rect,
    pub detail: Option<Rect>,
    /// Compact mode's one-line summary of the selection.
    pub summary: Option<Rect>,
}

impl Layout {
    pub fn compact(&self) -> bool {
        self.detail.is_none()
    }
}

pub fn layout(body: Rect) -> Layout {
    if body.width < SPLIT_MIN_WIDTH {
        let list_h = body.height.saturating_sub(2);
        return Layout {
            list: Rect {
                height: list_h,
                ..body
            },
            detail: None,
            summary: (body.height >= 3).then(|| {
                Rect::new(
                    body.x + 2,
                    body.bottom() - 1,
                    body.width.saturating_sub(2),
                    1,
                )
            }),
        };
    }
    // The list is the narrower column (~38%) so the detail pane gets the room.
    // u32: `width * 38` overflows u16 on very wide terminals.
    let list_w = (u32::from(body.width) * 38 / 100).clamp(LIST_MIN.into(), LIST_MAX.into()) as u16;
    let detail_x = body.x + list_w + GAP;
    Layout {
        list: Rect {
            width: list_w,
            ..body
        },
        detail: Some(Rect {
            x: detail_x,
            width: body.right() - detail_x,
            ..body
        }),
        summary: None,
    }
}

/// Where the list's rows go: below the title and a blank line, keeping the
/// last line free for the "↓ N more" hint.
pub fn list_rows(layout: &Layout) -> Rect {
    let pane = layout.list;
    Rect {
        y: pane.y + 2.min(pane.height),
        height: pane.height.saturating_sub(3),
        ..pane
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_the_breakpoint() {
        assert!(layout(Rect::new(2, 1, 75, 20)).compact());
        let split = layout(Rect::new(2, 1, 76, 20));
        let detail = split.detail.unwrap_or_default();
        assert_eq!(split.list.width, 30);
        assert_eq!(detail.x, 2 + 30 + 2);
        assert_eq!(detail.right(), 78);
    }

    #[test]
    fn list_column_is_clamped() {
        let wide = layout(Rect::new(0, 0, 200, 40));
        assert_eq!(wide.list.width, LIST_MAX);
        assert_eq!(wide.detail.unwrap_or_default().width, 200 - LIST_MAX - GAP);
    }
}
