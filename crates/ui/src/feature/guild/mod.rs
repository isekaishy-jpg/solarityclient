//! Character guild membership and guild-roster presentation state.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{Lua, Table};

/// Shared server-published guild membership capability.
#[derive(Clone, Debug, Default)]
pub struct UiGuildState {
    member: Rc<Cell<bool>>,
    roster_selection: Rc<Cell<usize>>,
    roster_member_count: Rc<Cell<usize>>,
    message_of_the_day: Rc<RefCell<String>>,
}

impl UiGuildState {
    /// Creates the pre-character state with no guild membership.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces whether the active character belongs to a guild.
    pub fn set_member(&self, member: bool) {
        self.member.set(member);
    }

    /// Replaces the known roster size used to validate client selection.
    pub fn set_roster_member_count(&self, member_count: usize) {
        self.roster_member_count.set(member_count);
        if self.roster_selection.get() > member_count {
            self.roster_selection.set(0);
        }
    }

    /// Replaces the last server-published guild message of the day.
    pub fn set_message_of_the_day(&self, message: impl Into<String>) {
        *self.message_of_the_day.borrow_mut() = message.into();
    }

    /// Reports whether the active character belongs to a guild.
    #[must_use]
    pub fn is_member(&self) -> bool {
        self.member.get()
    }

    /// Returns the one-based selected roster row or zero when no row is selected.
    #[must_use]
    pub fn roster_selection(&self) -> usize {
        self.roster_selection.get()
    }
}

/// Registers guild capability queries consumed by the social frame.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiGuildState,
) -> mlua::Result<()> {
    let selection_state = state.clone();
    let get_selection_state = state.clone();
    let motd_state = state.clone();
    globals.raw_set(
        "IsInGuild",
        lua.create_function(move |_, ()| Ok(state.is_member().then_some(1_u8)))?,
    )?;
    globals.raw_set(
        "SetGuildRosterSelection",
        lua.create_function(move |lua, value: mlua::Value| {
            let number = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: SetGuildRosterSelection(index)"))?;
            let index = if number.is_finite() && number >= 1.0 && number <= usize::MAX as f64 {
                number.round() as usize
            } else {
                0
            };
            selection_state.roster_selection.set(
                if index <= selection_state.roster_member_count.get() {
                    index
                } else {
                    0
                },
            );
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "GetGuildRosterSelection",
        lua.create_function(move |_, ()| Ok(get_selection_state.roster_selection()))?,
    )?;
    globals.raw_set(
        "GetGuildRosterMOTD",
        lua.create_function(move |lua, ()| {
            lua.create_string(motd_state.message_of_the_day.borrow().as_str())
        })?,
    )
}
