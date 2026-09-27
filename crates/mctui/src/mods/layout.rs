//! Mods screen geometry: pure functions of the body area.

use ratatui::layout::Rect;

use super::Pane;

/// At or above this body width Installed and Discover sit side by side;
/// below it one pane shows at a time. 76 is an 80-column terminal's body.
pub const SPLIT_MIN_WIDTH: u16 = 76;
/// At or above this body width the selected result's details get a column
/// of their own instead of a block under the results.
pub const THREE_MIN_WIDTH: u16 = 118;

/// Between panes; with the 2-column inset of each pane's text, 4 columns of
/// air separate one pane's bands from the next pane's words.
const GAP: u16 = 2;
/// Rows the details under the results take, with the blank above them: in
/// full (title, who and what, two lines of summary, stats, a blank, the
/// buttons), or brief (one line of summary and no stats) when room is short.
pub const DETAILS_FULL: u16 = 8;
pub const DETAILS_BRIEF: u16 = 6;
/// The fewest rows the results keep before the details give way: three
/// results and the "↓ N more" line.
const MIN_RESULTS: u16 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// `None` when compact and Discover is showing.
    pub installed: Option<Rect>,
    /// `None` when compact and Installed is showing.
    pub discover: Option<Rect>,
    /// The details column, when there's room for one.
    pub details: Option<Rect>,
}

pub fn compact(body: Rect) -> bool {
    body.width < SPLIT_MIN_WIDTH
}

/// Where the panes go. When compact, only `shown` gets the body.
pub fn layout(body: Rect, shown: Pane) -> Layout {
    if compact(body) {
        let (installed, discover) = match shown {
            Pane::Installed => (Some(body), None),
            Pane::Discover => (None, Some(body)),
        };
        return Layout {
            installed,
            discover,
            details: None,
        };
    }
    // u32: `width * pct` overflows u16 on very wide terminals.
    let share = |pct: u32, min: u16, max: u16| {
        (u32::from(body.width) * pct / 100).clamp(min.into(), max.into()) as u16
    };
    let three = body.width >= THREE_MIN_WIDTH;
    let installed_w = if three {
        share(26, 30, 40)
    } else {
        share(34, 26, 44)
    };
    // Room for the Install and Modrinth page buttons side by side.
    let details_w = if three { share(30, 40, 52) } else { 0 };
    let discover_x = body.x + installed_w + GAP;
    let discover_w = body
        .right()
        .saturating_sub(discover_x)
        .saturating_sub(if three { details_w + GAP } else { 0 });
    let discover = Rect {
        x: discover_x,
        width: discover_w,
        ..body
    };
    Layout {
        installed: Some(Rect {
            width: installed_w,
            ..body
        }),
        discover: Some(discover),
        details: three.then(|| {
            let x = discover.right() + GAP;
            Rect {
                x,
                width: body.right().saturating_sub(x),
                ..body
            }
        }),
    }
}

/// The rows of the Installed pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstalledParts {
    pub label: Rect,
    /// The instance's version, level with Discover's search field.
    pub info: Rect,
    pub list: Rect,
}

pub fn installed(pane: Rect) -> InstalledParts {
    InstalledParts {
        label: row(pane, 0),
        info: row(pane, 2),
        list: odd(below(pane, 4)),
    }
}

/// The rows of the Discover pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoverParts {
    pub label: Rect,
    pub field: Rect,
    /// The install in progress, or the last install's warnings.
    pub activity: Rect,
    pub list: Rect,
    /// The selected result's details, under the results.
    pub details: Option<Rect>,
}

/// Discover's rows, with `activity` rows (plus a blank after them) above the
/// results, and the details block under them if `details` and it fits.
pub fn discover(pane: Rect, activity: u16, details: bool) -> DiscoverParts {
    let activity_area = Rect {
        height: activity,
        ..below(pane, 4)
    }
    .intersection(pane);
    let top = 4 + if activity > 0 { activity + 1 } else { 0 };
    let rest = below(pane, top);
    let size = [DETAILS_FULL, DETAILS_BRIEF]
        .into_iter()
        .find(|size| details && rest.height >= MIN_RESULTS + size);
    let details = size.map(|size| Rect {
        y: rest.bottom() - (size - 1),
        height: size - 1,
        ..rest
    });
    let list = Rect {
        height: rest.height - size.unwrap_or(0),
        ..rest
    };
    DiscoverParts {
        label: row(pane, 0),
        field: row(pane, 2),
        activity: activity_area,
        list: odd(list),
        details,
    }
}

/// `list` less its last row if it has an even number: a list keeps its last
/// row for "↓ N more", and an odd height leaves a blank line above that.
fn odd(list: Rect) -> Rect {
    Rect {
        height: list.height - u16::from(list.height > 0 && list.height.is_multiple_of(2)),
        ..list
    }
}

/// Row `dy` of `pane` (empty if the pane is shorter).
fn row(pane: Rect, dy: u16) -> Rect {
    Rect {
        height: 1,
        ..below(pane, dy)
    }
    .intersection(pane)
}

/// Everything from row `dy` of `pane` down.
fn below(pane: Rect, dy: u16) -> Rect {
    let dy = dy.min(pane.height);
    Rect {
        y: pane.y + dy,
        height: pane.height - dy,
        ..pane
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_shows_one_pane() {
        let body = Rect::new(2, 3, 75, 18);
        let lay = layout(body, Pane::Discover);
        assert_eq!((lay.installed, lay.discover), (None, Some(body)));
        let lay = layout(body, Pane::Installed);
        assert_eq!((lay.installed, lay.discover), (Some(body), None));
    }

    #[test]
    fn panes_share_the_width_and_gain_a_details_column() {
        let two = layout(Rect::new(2, 3, 76, 18), Pane::Discover);
        let (installed, discover) = (two.installed.unwrap(), two.discover.unwrap());
        assert_eq!(installed.width, 26);
        assert_eq!(discover.x, 2 + 26 + GAP);
        assert_eq!(discover.right(), 78);
        assert_eq!(two.details, None);

        let three = layout(Rect::new(2, 3, 130, 30), Pane::Discover);
        let (discover, details) = (three.discover.unwrap(), three.details.unwrap());
        assert_eq!(details.x, discover.right() + GAP);
        assert_eq!(details.right(), 132);
        assert!(discover.width >= 44, "{discover:?}");
        let narrowest = layout(Rect::new(0, 0, THREE_MIN_WIDTH, 30), Pane::Discover);
        assert!(narrowest.discover.unwrap().width >= 44, "{narrowest:?}");
    }

    #[test]
    fn the_details_block_needs_room() {
        let tall = discover(Rect::new(0, 0, 60, 24), 0, true);
        assert_eq!(tall.list, Rect::new(0, 4, 60, 11), "an odd height");
        assert_eq!(tall.details, Some(Rect::new(0, 17, 60, 7)));
        // An 80×24 terminal: brief details.
        let short = discover(Rect::new(0, 0, 60, 18), 0, true);
        assert_eq!(short.list, Rect::new(0, 4, 60, 7));
        assert_eq!(short.details, Some(Rect::new(0, 13, 60, 5)));
        // Installing: the progress rows push the results down, and the
        // details give way.
        let busy = discover(Rect::new(0, 0, 60, 18), 2, true);
        assert_eq!(busy.activity, Rect::new(0, 4, 60, 2));
        assert_eq!(busy.list, Rect::new(0, 7, 60, 11));
        assert_eq!(busy.details, None);
        assert_eq!(installed(Rect::new(0, 0, 30, 18)).list.height, 13);
    }

    #[test]
    fn tiny_panes_stay_inside() {
        let pane = Rect::new(5, 5, 10, 2);
        let parts = discover(pane, 3, true);
        for r in [parts.label, parts.field, parts.activity, parts.list] {
            assert!(r.is_empty() || pane.contains(r.as_position()), "{r:?}");
        }
    }
}
