//! Where the panes go: pure functions of the body area and the focus.

use ratatui::layout::Rect;

use super::Pane;

/// At or above this body width all three panes sit side by side.
pub const WIDE_MIN: u16 = 100;
/// At or above this body width the categories stay beside one view that
/// switches between the packs and your pack; below it, one pane at a time.
pub const SPLIT_MIN: u16 = 76;
const GAP: u16 = 2;
/// Rows the selected pack's details take under the packs list, gap included.
const DETAILS_H: u16 = 8;
/// The packs pane needs this many rows below its title before details fit.
const DETAILS_MIN: u16 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Categories, packs and your pack side by side.
    Wide,
    /// Categories, and the packs or your pack.
    Split,
    /// One pane at a time.
    Single,
}

impl Mode {
    pub fn of(body: Rect) -> Self {
        match body.width {
            WIDE_MIN.. => Mode::Wide,
            SPLIT_MIN.. => Mode::Split,
            _ => Mode::Single,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub mode: Mode,
    pub categories: Option<Rect>,
    pub packs: Option<Rect>,
    pub cart: Option<Rect>,
    /// The offline notice's row, at the bottom.
    pub notice: Option<Rect>,
}

/// The panes for `body`, showing `focus` wherever panes share a place, and
/// leaving the last row for a notice when there is one.
pub fn layout(body: Rect, focus: Pane, notice: bool) -> Layout {
    let (panes, notice) = if notice && body.height >= 8 {
        let row = Rect::new(
            body.x + 2,
            body.bottom() - 1,
            body.width.saturating_sub(4),
            1,
        );
        (
            Rect {
                height: body.height - 2,
                ..body
            },
            Some(row),
        )
    } else {
        (body, None)
    };
    let mode = Mode::of(body);
    let mut lay = Layout {
        mode,
        categories: None,
        packs: None,
        cart: None,
        notice,
    };
    match mode {
        Mode::Wide => {
            // u32: `width * 32` overflows u16 on very wide terminals.
            let share = |pct: u32, min: u16, max: u16| {
                (u32::from(panes.width) * pct / 100).clamp(min.into(), max.into()) as u16
            };
            let (categories_w, cart_w) = (share(22, 24, 30), share(32, 32, 44));
            let (left, rest) = split(panes, categories_w);
            let packs_w = rest.width.saturating_sub(cart_w + GAP);
            let (middle, right) = split(rest, packs_w);
            lay.categories = Some(left);
            lay.packs = Some(middle);
            lay.cart = Some(right);
        }
        Mode::Split => {
            let categories_w = (u32::from(panes.width) * 32 / 100).clamp(24, 30) as u16;
            let (left, right) = split(panes, categories_w);
            lay.categories = Some(left);
            if focus == Pane::Cart {
                lay.cart = Some(right);
            } else {
                lay.packs = Some(right);
            }
        }
        Mode::Single => match focus {
            Pane::Categories => lay.categories = Some(panes),
            Pane::Packs => lay.packs = Some(panes),
            Pane::Cart => lay.cart = Some(panes),
        },
    }
    lay
}

/// `area` cut into a `width`-wide left part and what's right of it after the
/// gutter.
fn split(area: Rect, width: u16) -> (Rect, Rect) {
    let width = width.min(area.width);
    let right_x = (area.x + width + GAP).min(area.right());
    (
        Rect { width, ..area },
        Rect {
            x: right_x,
            width: area.right() - right_x,
            ..area
        },
    )
}

/// A pane's content: below its title row and the blank line after it.
pub fn content(pane: Rect) -> Rect {
    let skip = 2.min(pane.height);
    Rect {
        y: pane.y + skip,
        height: pane.height - skip,
        ..pane
    }
}

/// The parts of the packs pane below its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacksArea {
    /// The category's warning, when it has one.
    pub warning: Option<Rect>,
    pub list: Rect,
    /// The selected pack's details, when there's room.
    pub details: Option<Rect>,
}

/// Splits the packs pane: `warning` lines, a blank, the list, then details.
pub fn packs_area(pane: Rect, warning: u16) -> PacksArea {
    let mut rest = content(pane);
    let details = (rest.height >= DETAILS_MIN).then(|| {
        rest.height -= DETAILS_H;
        Rect::new(rest.x, rest.bottom() + 1, rest.width, DETAILS_H - 1)
    });
    let warning = (warning > 0 && rest.height > warning + 3).then(|| {
        let area = Rect {
            height: warning,
            ..rest
        };
        rest.y += warning + 1;
        rest.height -= warning + 1;
        area
    });
    PacksArea {
        warning,
        list: rest,
        details,
    }
}

/// The parts of the cart pane below its title: the list, and `footer` rows
/// at the bottom for the build button and its notes.
pub fn cart_area(pane: Rect, footer: u16) -> (Rect, Option<Rect>) {
    let rest = content(pane);
    if footer == 0 || rest.height <= footer + 3 {
        return (rest, None);
    }
    let list = Rect {
        height: rest.height - footer,
        ..rest
    };
    (
        list,
        Some(Rect::new(rest.x, list.bottom(), rest.width, footer)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_as_the_width_shrinks() {
        let wide = layout(Rect::new(2, 3, 116, 24), Pane::Packs, false);
        let (c, p, r) = (
            wide.categories.unwrap_or_default(),
            wide.packs.unwrap_or_default(),
            wide.cart.unwrap_or_default(),
        );
        assert_eq!((c.x, c.width), (2, 25));
        assert_eq!(p.x, c.right() + GAP);
        assert_eq!(r.x, p.right() + GAP);
        assert_eq!(r.right(), 118, "the cart runs to the edge");

        let split = layout(Rect::new(0, 0, 90, 24), Pane::Cart, false);
        assert_eq!(split.mode, Mode::Split);
        assert!(split.packs.is_none() && split.cart.is_some());

        let single = layout(Rect::new(0, 0, 60, 24), Pane::Packs, true);
        assert_eq!(single.packs.map(|p| p.height), Some(22));
        assert_eq!(single.notice.map(|n| n.y), Some(23));
        assert!(single.categories.is_none());
    }

    #[test]
    fn details_need_room() {
        let tall = packs_area(Rect::new(0, 0, 50, 30), 2);
        assert_eq!(tall.warning.map(|w| (w.y, w.height)), Some((2, 2)));
        assert_eq!(tall.list.y, 5);
        let details = tall.details.unwrap_or_default();
        assert_eq!(details.bottom(), 30);
        assert_eq!(tall.list.bottom() + 1, details.y);
        assert!(packs_area(Rect::new(0, 0, 50, 12), 0).details.is_none());
    }
}
