//! External stock-compatibility tests for client-owned action-bar state.

use solarity_ui::{UiActionBarPageError, UiActionBarState, UiActionBarStateError};

/// The primary bar starts on page one and accepts only stock's six pages.
#[test]
fn primary_page_preserves_the_closed_stock_range() {
    let state = UiActionBarState::new();
    assert_eq!(state.bonus_bar_offset(), 0);
    state.set_bonus_bar_offset(5);
    assert_eq!(state.bonus_bar_offset(), 5);
    assert_eq!(state.page(), 1);
    assert_eq!(state.set_page(6), Ok(()));
    assert_eq!(state.page(), 6);
    assert_eq!(state.set_page(0), Err(UiActionBarPageError { page: 0 }));
    assert_eq!(state.page(), 6);
    assert_eq!(state.set_page(7), Err(UiActionBarPageError { page: 7 }));
    assert_eq!(state.page(), 6);
}

/// Stock's zero-initialized process image precedes the complete server image.
#[test]
fn action_slots_start_empty_and_accept_a_complete_server_image() {
    let state = UiActionBarState::new();
    assert_eq!(state.toggles(), [false; 4]);
    state.set_toggles([true, false, true, false]);
    assert_eq!(state.toggles(), [true, false, true, false]);
    assert_eq!(state.packed_slot(1), Ok(0));
    assert_eq!(state.packed_slot(144), Ok(0));

    let mut slots = [0_u32; 144];
    slots[0] = 0x8000_1234;
    slots[143] = 0x0000_5678;
    state.set_slots(slots);
    assert_eq!(state.packed_slot(0), Ok(0));
    assert_eq!(state.packed_slot(1), Ok(0x8000_1234));
    assert_eq!(state.packed_slot(144), Ok(0x0000_5678));
    assert_eq!(state.packed_slot(145), Ok(0));

    state.clear_slots();
    assert_eq!(
        state.packed_slot(1),
        Err(UiActionBarStateError::Unavailable)
    );
}
