//! Outgoing mail composition state projected into FrameXML.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Lua, Table};

/// Build-12340 postage for an empty message, in copper.
pub const UI_BASE_SEND_MAIL_PRICE: u32 = 30;

/// Shared server-compatible postage quote for the current outgoing message.
#[derive(Clone, Debug)]
pub struct UiMailComposeState {
    postage: Rc<Cell<u32>>,
}

impl Default for UiMailComposeState {
    fn default() -> Self {
        Self {
            postage: Rc::new(Cell::new(UI_BASE_SEND_MAIL_PRICE)),
        }
    }
}

impl UiMailComposeState {
    /// Creates an empty compose state at the native 30-copper base price.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the quote after attachments and transferred money change.
    pub fn set_postage(&self, postage: u32) {
        self.postage.set(postage);
    }

    /// Returns the current postage quote in copper.
    #[must_use]
    pub fn postage(&self) -> u32 {
        self.postage.get()
    }
}

/// Registers outgoing-mail queries consumed during mail-frame setup.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiMailComposeState,
) -> mlua::Result<()> {
    globals.raw_set(
        "GetSendMailPrice",
        lua.create_function(move |_, ()| Ok(state.postage()))?,
    )
}
