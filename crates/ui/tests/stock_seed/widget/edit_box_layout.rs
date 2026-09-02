//! External regressions for retained EditBox pointer geometry.

#[path = "../../../src/font/edit_box_layout.rs"]
mod edit_box_layout;

use edit_box_layout::EditBoxTextLayout;

/// Proportional cluster midpoints map to exact UTF-8 byte boundaries.
#[test]
fn pointer_cursor_uses_retained_cluster_midpoints() {
    let mut layout = EditBoxTextLayout::new(5, 5, [5, 5]);
    layout.reserve_lines(1);
    layout.push_line(-12.0);
    layout.push_cluster(0, 1, 0, 4.0, 10.0);
    layout.push_cluster(1, 3, 0, 10.0, 22.0);
    layout.push_cluster(3, 5, 0, 22.0, 30.0);

    assert_eq!(layout.cursor_at((4.0, -12.0)), Some(0));
    assert_eq!(layout.cursor_at((6.9, -12.0)), Some(0));
    assert_eq!(layout.cursor_at((7.0, -12.0)), Some(1));
    assert_eq!(layout.cursor_at((15.9, -12.0)), Some(1));
    assert_eq!(layout.cursor_at((16.0, -12.0)), Some(3));
    assert_eq!(layout.cursor_at((40.0, -12.0)), Some(5));
}

/// The nearest shaped line wins and an empty line resolves to buffer end.
#[test]
fn pointer_cursor_selects_nearest_line_and_handles_empty_lines() {
    let mut layout = EditBoxTextLayout::new(4, 0, [0, 0]);
    layout.push_line(-10.0);
    layout.push_line(-30.0);
    layout.push_cluster(0, 2, 0, 0.0, 10.0);

    assert_eq!(layout.cursor_at((0.0, -9.0)), Some(0));
    assert_eq!(layout.cursor_at((0.0, -29.0)), Some(4));
}

/// Drag and shift-click extension retain the endpoint opposite the cursor.
#[test]
fn selection_anchor_is_opposite_live_cursor() {
    assert_eq!(EditBoxTextLayout::new(8, 3, [3, 3]).selection_anchor(), 3);
    assert_eq!(EditBoxTextLayout::new(8, 2, [2, 7]).selection_anchor(), 7);
    assert_eq!(EditBoxTextLayout::new(8, 7, [2, 7]).selection_anchor(), 2);
}
