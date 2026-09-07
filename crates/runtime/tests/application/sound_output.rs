//! Native saved output selection under missing and reordered device lists.

use super::resolve_index;

/// 8790C0 keeps matching duplicates at the saved index, searches renamed order
/// case-insensitively, and chooses default when the saved physical device left.
#[test]
fn saved_output_name_survives_reordering_and_disconnect() {
    let names = ["System Default", "Speakers", "Headphones", "Speakers"];
    assert_eq!(resolve_index(&names, 3, "Speakers"), 3);
    assert_eq!(resolve_index(&names, 1, "HEADPHONES"), 2);
    assert_eq!(resolve_index(&names, 9, "Speakers"), 1);
    assert_eq!(resolve_index(&names, -1, "Headphones"), 2);
    assert_eq!(resolve_index(&names, 2, "Removed output"), 0);
    assert_eq!(resolve_index(&names, 1, "System Default"), 0);
    assert_eq!(resolve_index(&["System Default"], 2, "Headphones"), 0);
}
