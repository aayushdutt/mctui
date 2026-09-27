//! Fuzzy matching for filters and the command palette.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// One matching item: its index in the input and the character positions that
/// matched (for highlighting).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub index: usize,
    pub positions: Vec<u32>,
}

/// Ranks `items` against `query`, best first. An empty query keeps every item
/// in its original order.
pub fn rank<'a>(query: &str, items: impl IntoIterator<Item = &'a str>) -> Vec<Match> {
    let query = query.trim();
    if query.is_empty() {
        return items
            .into_iter()
            .enumerate()
            .map(|(index, _)| Match {
                index,
                positions: Vec::new(),
            })
            .collect();
    }
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, Match)> = items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let mut positions = Vec::new();
            let score =
                pattern.indices(Utf32Str::new(item, &mut buf), &mut matcher, &mut positions)?;
            positions.sort_unstable();
            positions.dedup();
            Some((score, Match { index, positions }))
        })
        .collect();
    // Stable: equal scores keep their original order.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, m)| m).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_keeps_everything_in_order() {
        let got: Vec<usize> = rank("  ", ["b", "a"])
            .into_iter()
            .map(|m| m.index)
            .collect();
        assert_eq!(got, [0, 1]);
    }

    #[test]
    fn ranks_and_reports_positions() {
        let items = ["Resource packs", "Mods", "Settings"];
        let got = rank("mod", items);
        assert_eq!(got[0].index, 1);
        assert_eq!(got[0].positions, [0, 1, 2]);
        assert!(rank("zzz", items).is_empty());
    }
}
