//! Locale-loose credits text exposed to stock GlueXML.

use mlua::{Lua, Table, Value};
use solarity_asset::{AssetStoreHandle, LocalizedDocument};

/// Registers build-12340's `GetCreditsText` native.
///
/// Wow.exe `FUN_004DCD60` selects the base, Burning Crusade, or Wrath locale
/// file for indices one, two, or every other integer respectively. A missing
/// file leaves the incoming argument at the Lua return slot, matching the
/// native function's unchanged stack on its failed load path.
pub(super) fn register_globals(
    lua: &Lua,
    globals: &Table,
    assets: Option<AssetStoreHandle>,
) -> mlua::Result<()> {
    globals.raw_set(
        "GetCreditsText",
        lua.create_function(move |lua, argument: Value| {
            let index = lua
                .coerce_number(argument.clone())?
                .ok_or_else(|| mlua::Error::runtime("Usage: Script_GetCreditText(versionIndex)"))?
                as i32;
            let document = match index {
                1 => LocalizedDocument::Credits,
                2 => LocalizedDocument::CreditsBurningCrusade,
                _ => LocalizedDocument::CreditsWrath,
            };
            let Some(assets) = assets.as_ref() else {
                return Ok(argument);
            };
            let bytes = assets
                .borrow()
                .read_localized_document(document)
                .map_err(mlua::Error::external)?;
            match bytes {
                Some(bytes) => Ok(Value::String(lua.create_string(bytes)?)),
                None => Ok(argument),
            }
        })?,
    )
}
