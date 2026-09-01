//! Battlefield queue state projected from build-12340 world-session packets.

mod state;

use mlua::{Lua, MultiValue, Table, Value};

pub use state::{
    MAX_BATTLEFIELD_QUEUES, MAX_WORLD_PVP_QUEUES, UiBattlefieldQueueError, UiBattlefieldQueueState,
    UiBattlefieldQueueStatus, UiBattlefieldSlot, UiBattlegroundType, UiWorldPvpQueueSlot,
};

/// Registers the seven-value build-12340 battlefield status query.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiBattlefieldQueueState,
) -> mlua::Result<()> {
    let battlefield = state.clone();
    let battleground_count = state.clone();
    let battleground_info = state.clone();
    globals.raw_set(
        "GetBattlefieldStatus",
        lua.create_function(move |_, index: usize| {
            let slot = battlefield.slot(index).map_err(|_| {
                mlua::Error::runtime(format!(
                    "GetBattlefieldStatus index must be in 1..={MAX_BATTLEFIELD_QUEUES}"
                ))
            })?;
            Ok((
                slot.status().as_str(),
                slot.map_name().map(str::to_owned),
                slot.instance_id(),
                slot.minimum_level(),
                slot.maximum_level(),
                slot.team_size(),
                slot.registered_match(),
            ))
        })?,
    )?;
    globals.raw_set(
        "GetWorldPVPQueueStatus",
        lua.create_function(move |_, index: usize| {
            let slot = state.world_pvp_slot(index).map_err(|_| {
                mlua::Error::runtime(format!(
                    "GetWorldPVPQueueStatus index must be in 1..={MAX_WORLD_PVP_QUEUES}"
                ))
            })?;
            Ok((
                slot.status().as_str(),
                slot.map_name().map(str::to_owned),
                slot.queue_id(),
                slot.expiration_milliseconds(),
            ))
        })?,
    )?;
    globals.raw_set(
        "GetNumBattlegroundTypes",
        lua.create_function(move |_, ()| Ok(battleground_count.battleground_type_count()))?,
    )?;
    globals.raw_set(
        "GetBattlegroundInfo",
        lua.create_function(move |lua, index: usize| {
            let Some(battleground) = battleground_info.battleground_type(index) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(battleground.name)?),
                Value::Boolean(battleground.can_enter),
                Value::Boolean(battleground.holiday),
                Value::Boolean(battleground.random),
                Value::Integer(i64::from(battleground.battleground_id)),
            ]))
        })?,
    )
}
