//! Bag and container-frame presentation behavior evidenced by `ContainerFrame.cpp`.

mod container_frame;

use mlua::{Lua, MultiValue, Table, Value};

use crate::UiWorldState;

/// Registers the stock empty-container query used before inventory publication.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    world: UiWorldState,
) -> mlua::Result<()> {
    globals.raw_set(
        "GetContainerNumFreeSlots",
        lua.create_function(move |_, index: i32| {
            // Script_GetContainerNumFreeSlots at 0x005D7590 counts all sixteen
            // backpack fields as empty for a resolved player whose inventory
            // update contains no items. Missing bag objects return one zero.
            if index == 0 && world.player().is_some() {
                return Ok(MultiValue::from_vec(vec![
                    Value::Integer(16),
                    Value::Integer(0),
                ]));
            }
            Ok(MultiValue::from_vec(vec![Value::Integer(0)]))
        })?,
    )?;
    globals.raw_set(
        "GetInventoryAlertStatus",
        lua.create_function(|_, _index: u32| {
            // Script_GetInventoryAlertStatus at 0x005E7FA0 returns zero for
            // every slot outside the current durability-alert array.
            Ok(0_u32)
        })?,
    )?;
    globals.raw_set(
        "OffhandHasWeapon",
        lua.create_function(|_, ()| {
            // Script_OffhandHasWeapon at 0x005EAC10 returns nil unless the
            // offhand inventory object exists and has weapon inventory type.
            Ok(Option::<u8>::None)
        })?,
    )?;
    globals.raw_set(
        "GetNumBankSlots",
        lua.create_function(|_, ()| {
            // Script_GetNumBankSlots at 0x0057BAB0 returns the purchased bag
            // count followed by true only after all seven slots are owned.
            Ok(MultiValue::from_vec(vec![Value::Integer(0), Value::Nil]))
        })?,
    )
}
