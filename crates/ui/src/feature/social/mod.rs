//! Social-directory query results and client routing state.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Lua, Table, Value};

/// Shared destination policy for server `/who` results.
#[derive(Clone, Debug, Default)]
pub struct UiSocialQueryState {
    who_results_to_ui: Rc<Cell<bool>>,
}

impl UiSocialQueryState {
    /// Creates the stock chat-routed query state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reports whether new `/who` results are routed to WhoFrame.
    #[must_use]
    pub fn who_results_to_ui(&self) -> bool {
        self.who_results_to_ui.get()
    }
}

/// Registers social query routing retained by the client process.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiSocialQueryState,
) -> mlua::Result<()> {
    globals.raw_set(
        "SetWhoToUI",
        lua.create_function(move |_, value: Option<Value>| {
            state.who_results_to_ui.set(
                value.is_some_and(|value| !matches!(value, Value::Nil | Value::Boolean(false))),
            );
            Ok(())
        })?,
    )
}
