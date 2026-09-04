//! Customer-support request state projected between FrameXML and networking.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Lua, Table};

/// Pending world-session support requests emitted by built-in FrameXML.
#[derive(Clone, Debug, Default)]
pub struct UiSupportState {
    gm_ticket_requested: Rc<Cell<bool>>,
}

impl UiSupportState {
    /// Creates an empty support-request mailbox.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the pending GM-ticket refresh request.
    pub fn take_gm_ticket_request(&self) -> bool {
        self.gm_ticket_requested.replace(false)
    }
}

/// Registers stock customer-support request boundaries.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiSupportState,
) -> mlua::Result<()> {
    globals.raw_set(
        "GetGMTicket",
        lua.create_function(move |_, ()| {
            // Script_GetGMTicket is the 0x005AD070 thunk; its target at
            // 0x005ACBF0 sends build-12340 opcode 0x211 without a payload.
            state.gm_ticket_requested.set(true);
            Ok(())
        })?,
    )
}
