//! Character equipment, inventory art, item portrait, and paper-doll information presentation.

mod paper_doll_info_frame;

use mlua::{Lua, MultiValue, Table, Value};
use solarity_asset::{AssetStoreHandle, PaperDollItemFrameCatalog};

/// Registers inventory-slot metadata from the mounted client-authored DBC.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    assets: Option<AssetStoreHandle>,
) -> mlua::Result<()> {
    let catalog = assets
        .map(|assets| PaperDollItemFrameCatalog::load(&mut assets.borrow_mut()))
        .transpose()
        .map_err(|error| mlua::Error::runtime(error.to_string()))?;
    globals.raw_set(
        "GetInventorySlotInfo",
        lua.create_function(move |lua, value: Value| {
            let name = lua
                .coerce_string(value)?
                .ok_or_else(invalid_inventory_slot)?;
            let definition = catalog
                .as_ref()
                .and_then(|catalog| catalog.definition(&name.to_string_lossy()))
                .ok_or_else(invalid_inventory_slot)?;
            Ok(MultiValue::from_vec(vec![
                Value::Integer(i64::from(definition.slot_number())),
                Value::String(lua.create_string(definition.slot_icon())?),
                if definition.slot_number() == 18 {
                    Value::Number(1.0)
                } else {
                    Value::Nil
                },
            ]))
        })?,
    )
}

fn invalid_inventory_slot() -> mlua::Error {
    mlua::Error::runtime("Invalid inventory slot in GetInventorySlotInfo")
}
