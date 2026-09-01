//! Authenticated account entitlements exposed to stock feature frames.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Lua, Table};

/// Highest content entitlement returned for the authenticated account.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiAccountExpansion {
    /// Original World of Warcraft content only.
    #[default]
    Original,
    /// The Burning Crusade content.
    TheBurningCrusade,
    /// Wrath of the Lich King content.
    WrathOfTheLichKing,
}

impl UiAccountExpansion {
    const fn level(self) -> u8 {
        match self {
            Self::Original => 0,
            Self::TheBurningCrusade => 1,
            Self::WrathOfTheLichKing => 2,
        }
    }
}

/// Shared account entitlement image updated from world authentication.
#[derive(Clone, Debug, Default)]
pub struct UiAccountState {
    expansion: Rc<Cell<UiAccountExpansion>>,
}

impl UiAccountState {
    /// Creates the stock pre-authentication entitlement image.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the expansion level with the value returned by the world server.
    pub fn set_expansion(&self, expansion: UiAccountExpansion) {
        self.expansion.set(expansion);
    }

    /// Returns the account's highest authenticated content entitlement.
    #[must_use]
    pub fn expansion(&self) -> UiAccountExpansion {
        self.expansion.get()
    }
}

/// Registers account APIs backed by authenticated session state.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiAccountState,
) -> mlua::Result<()> {
    globals.raw_set(
        "GetAccountExpansionLevel",
        lua.create_function(move |_, ()| Ok(state.expansion().level()))?,
    )
}
