//! External stock-compatibility tests for client-owned action-bar state.

use solarity_ui::{UiActionBarPageError, UiActionBarState};

/// The primary bar starts on page one and accepts only stock's six pages.
#[test]
fn primary_page_preserves_the_closed_stock_range() {
    let state = UiActionBarState::new();
    assert_eq!(state.page(), 1);
    assert_eq!(state.set_page(6), Ok(()));
    assert_eq!(state.page(), 6);
    assert_eq!(state.set_page(0), Err(UiActionBarPageError { page: 0 }));
    assert_eq!(state.page(), 6);
    assert_eq!(state.set_page(7), Err(UiActionBarPageError { page: 7 }));
    assert_eq!(state.page(), 6);
}
