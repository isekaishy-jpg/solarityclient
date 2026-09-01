//! Action-slot layout, input, cooldown, and presentation behavior evidenced by `ActionBarFrame.cpp`.

mod action_bar_frame;
mod state;

use mlua::{Lua, Table};

pub use state::{UiActionBarPageError, UiActionBarState};

/// Registers globals backed by client-owned action-bar state.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiActionBarState,
) -> mlua::Result<()> {
    let change_state = state.clone();
    globals.raw_set(
        "GetActionBarPage",
        lua.create_function(move |_, ()| Ok(state.page()))?,
    )?;
    globals.raw_set(
        "ChangeActionBarPage",
        lua.create_function(move |_, page: u8| {
            change_state.set_page(page).map_err(|_| {
                mlua::Error::runtime("ChangeActionBarPage() needs a page in the range 1 to 6")
            })
        })?,
    )
}
