//! Home screen tests.

use crossterm::event::KeyCode;

use crate::app::tests::Harness;

#[test]
fn a_long_list_scrolls_and_says_how_much_more() {
    // Three rows fit: the fixture's instances, with "+ New instance" below.
    let mut h = Harness::new(80, 14);
    insta::assert_snapshot!("scroll_top", h.text());
    for _ in 0..3 {
        h.press(KeyCode::Down);
    }
    assert_eq!(h.selected(), None, "on + New instance");
    insta::assert_snapshot!("scroll_bottom", h.text());
    // Filtering hides "+ New instance": the selection stays at the end.
    h.press(KeyCode::Char('/'));
    h.type_text("s");
    insta::assert_snapshot!("scroll_filtered", h.text());
}
