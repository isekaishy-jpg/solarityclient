//! Retained client-owned action-bar page state.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use thiserror::Error;

/// Number of packed action slots sent by build 12340.
pub const UI_ACTION_SLOT_COUNT: usize = 144;

/// Shared action-bar page selected by FrameXML or bound input.
#[derive(Clone, Debug)]
pub struct UiActionBarState {
    page: Rc<Cell<u8>>,
    bonus_bar_offset: Rc<Cell<u8>>,
    slots: Rc<RefCell<Option<Box<[u32; UI_ACTION_SLOT_COUNT]>>>>,
}

impl Default for UiActionBarState {
    fn default() -> Self {
        Self {
            // Build 12340 initializes the primary action bar to page one.
            page: Rc::new(Cell::new(1)),
            bonus_bar_offset: Rc::new(Cell::new(0)),
            slots: Rc::new(RefCell::new(None)),
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

    /// Replaces the controlled-unit bonus action-bar page offset.
    pub fn set_bonus_bar_offset(&self, offset: u8) {
        self.bonus_bar_offset.set(offset);
    }

    /// Returns the controlled-unit bonus action-bar page offset.
    #[must_use]
    pub fn bonus_bar_offset(&self) -> u8 {
        self.bonus_bar_offset.get()
    }

    /// Publishes one complete packed server slot image.
    pub fn set_slots(&self, slots: [u32; UI_ACTION_SLOT_COUNT]) {
        *self.slots.borrow_mut() = Some(Box::new(slots));
    }

    /// Clears the server image when the active world ends.
    pub fn clear_slots(&self) {
        *self.slots.borrow_mut() = None;
    }

    /// Returns one packed Lua-facing slot, treating out-of-range indexes as empty.
    ///
    /// # Errors
    ///
    /// Returns [`UiActionBarStateError`] until the world server has supplied a
    /// complete slot image.
    pub fn packed_slot(&self, one_based_slot: i32) -> Result<u32, UiActionBarStateError> {
        let slots = self.slots.borrow();
        let slots = slots.as_ref().ok_or(UiActionBarStateError::Unavailable)?;
        let Some(index) = one_based_slot
            .checked_sub(1)
            .and_then(|slot| usize::try_from(slot).ok())
        else {
            return Ok(0);
        };
        Ok(slots.get(index).copied().unwrap_or(0))
    }
}

/// Invalid primary action-bar page supplied by script or input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("action-bar page {page} is outside the stock range 1..=6")]
pub struct UiActionBarPageError {
    /// Rejected one-byte page value.
    pub page: u8,
}

/// Action-slot query attempted before the world supplied its complete image.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UiActionBarStateError {
    /// `SMSG_ACTION_BUTTONS` has not yet established slot presence.
    #[error("action slots require an authoritative world-server image")]
    Unavailable,
}
