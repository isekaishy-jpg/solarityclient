//! Guild-tabard vendor session projected into FrameXML.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Lua, Table};

/// Shared price and lifecycle state for the active tabard-creation vendor.
#[derive(Clone, Debug, Default)]
pub struct UiTabardState {
    creation_cost: Rc<Cell<u32>>,
    active: Rc<Cell<bool>>,
}

impl UiTabardState {
    /// Creates the disconnected no-vendor state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the server-supplied tabard creation price and session state.
    pub fn set_session(&self, active: bool, creation_cost: u32) {
        self.active.set(active);
        self.creation_cost.set(creation_cost);
    }

    /// Returns whether a tabard vendor session remains active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.get()
    }
}

/// Registers the complete build-12340 tabard global family.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiTabardState,
) -> mlua::Result<()> {
    let close_state = state.clone();
    globals.raw_set(
        "GetTabardCreationCost",
        lua.create_function(move |_, ()| Ok(state.creation_cost.get()))?,
    )?;
    globals.raw_set(
        "CloseTabardCreation",
        lua.create_function(move |_, ()| {
            close_state.active.set(false);
            Ok(())
        })?,
    )
}
