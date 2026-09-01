//! Retained client-owned action-bar page state.

use std::cell::Cell;
use std::rc::Rc;

use thiserror::Error;

/// Shared action-bar page selected by FrameXML or bound input.
#[derive(Clone, Debug)]
pub struct UiActionBarState {
    page: Rc<Cell<u8>>,
}

impl Default for UiActionBarState {
    fn default() -> Self {
        Self {
            // Build 12340 initializes the primary action bar to page one.
            page: Rc::new(Cell::new(1)),
        }
    }
}

impl UiActionBarState {
    /// Creates the stock client-owned initial page state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the selected one-based primary action-bar page.
    #[must_use]
    pub fn page(&self) -> u8 {
        self.page.get()
    }

    /// Selects one of the six pages accepted by `ChangeActionBarPage`.
    ///
    /// # Errors
    ///
    /// Returns [`UiActionBarPageError`] when `page` is outside `1..=6`.
    pub fn set_page(&self, page: u8) -> Result<(), UiActionBarPageError> {
        if !(1..=6).contains(&page) {
            return Err(UiActionBarPageError { page });
        }
        self.page.set(page);
        Ok(())
    }
}

/// Invalid primary action-bar page supplied by script or input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("action-bar page {page} is outside the stock range 1..=6")]
pub struct UiActionBarPageError {
    /// Rejected one-byte page value.
    pub page: u8,
}
