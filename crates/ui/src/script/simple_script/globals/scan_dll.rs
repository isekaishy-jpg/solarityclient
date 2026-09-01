//! Native ScanDLL state queried by stock AccountLogin Lua.

use mlua::{Lua, Table};

/// Registers the completion query for the legacy launcher scan stage.
///
/// Solarity does not start the Win32 ScanDLL job, so its initial and terminal
/// state are the same: no scan remains in flight. This matches the stock query
/// contract and allows AccountLogin Lua to advance only after every authored
/// agreement has been accepted.
pub(super) fn register_globals(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    globals.raw_set("IsScanDLLFinished", lua.create_function(|_, ()| Ok(true))?)
}
