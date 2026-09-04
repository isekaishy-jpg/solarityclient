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
    toggles: Rc<Cell<[bool; 4]>>,
    slots: Rc<RefCell<Option<Box<[u32; UI_ACTION_SLOT_COUNT]>>>>,
}

impl Default for UiActionBarState {
    fn default() -> Self {
        Self {
            // Build 12340 initializes the primary action bar to page one.
            page: Rc::new(Cell::new(1)),
            bonus_bar_offset: Rc::new(Cell::new(0)),
            toggles: Rc::new(Cell::new([false; 4])),
            // Wow.exe FUN_006D8750 reads and updates the process-lifetime
            // DAT_00AD9F6C array in place. Its BSS image is already 144 zero
            // slots before the first SMSG_ACTION_BUTTONS packet arrives.
            slots: Rc::new(RefCell::new(Some(Box::new([0; UI_ACTION_SLOT_COUNT])))),
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

    /// Returns the four server-backed multi-action-bar visibility toggles.
    #[must_use]
    pub fn toggles(&self) -> [bool; 4] {
        self.toggles.get()
    }

    /// Replaces the four multi-action-bar visibility toggles.
    pub fn set_toggles(&self, toggles: [bool; 4]) {
        self.toggles.set(toggles);
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
    /// Returns [`UiActionBarStateError`] after world teardown has explicitly
    /// cleared the process-lifetime image.
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

/// Action-slot query attempted after the active world cleared its image.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UiActionBarStateError {
    /// The active-world owner has torn down its retained slot storage.
    #[error("action slots are unavailable outside an active world")]
    Unavailable,
}
