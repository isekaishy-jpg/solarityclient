//! External stock-compatibility tests for FrameXML physical input state.

use solarity_ui::{UiModifierKeyState, UiModifierKeys};

/// Side-specific keys compose generic modifiers and clear as one input image.
#[test]
fn modifier_state_preserves_physical_sides() {
    let state = UiModifierKeyState::new();
    assert_eq!(state.keys(), UiModifierKeys::default());

    let keys = UiModifierKeys::new(true, false, false, true, true, false);
    state.set(keys);
    assert_eq!(state.keys(), keys);
    assert!(keys.left_shift());
    assert!(!keys.right_shift());
    assert!(keys.shift());
    assert!(!keys.left_control());
    assert!(keys.right_control());
    assert!(keys.control());
    assert!(keys.left_alt());
    assert!(!keys.right_alt());
    assert!(keys.alt());

    state.clear();
    assert_eq!(state.keys(), UiModifierKeys::default());
}
