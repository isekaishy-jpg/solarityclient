//! Battlefield queue state projected from build-12340 world-session packets.

mod state;

use mlua::{Lua, MultiValue, Table, Value, Variadic};

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
    let active_arena = state.clone();
    let battlefield_position_requests = state.clone();
    let battleground_count = state.clone();
    let battleground_info = state.clone();
    let battlefield_count = state.clone();
    let battlefield_selection = state.clone();
    let battlefield_info = state.clone();
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
        "IsActiveBattlefieldArena",
        lua.create_function(move |_, ()| {
            let (arena, registered) = active_arena.active_arena();
            Ok((arena.then_some(1), registered.then_some(1)))
        })?,
    )?;
    globals.raw_set(
        "RequestBattlefieldPositions",
        lua.create_function(move |_, _arguments: Variadic<Value>| {
            // The build-12340 wrapper at 0x0054DCB0 ignores Lua arguments.
            // Its core at 0x0054CF60 immediately returns unless a battlefield
            // is active; only that active branch emits the throttled position
            // request. Preserve the constant-time ordinary-world path here.
            let _has_active_battlefield = (1..=MAX_BATTLEFIELD_QUEUES).any(|index| {
                battlefield_position_requests
                    .slot(index)
                    .is_ok_and(|slot| slot.status() == UiBattlefieldQueueStatus::Active)
            });
            Ok(())
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
        "GetNumBattlefields",
        lua.create_function(move |_, ()| Ok(battlefield_count.battleground_type_count()))?,
    )?;
    globals.raw_set(
        "SetSelectedBattlefield",
        lua.create_function(move |_, index: usize| {
            // The stock battlefield list is explicitly zero based.
            battlefield_selection.set_selected_battleground(index);
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "GetBattlefieldInfo",
        lua.create_function(move |lua, ()| {
            let Some(battleground) = battlefield_info.selected_battleground() else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(&battleground.name)?),
                Value::String(lua.create_string(&battleground.description)?),
                Value::Integer(i64::from(battleground.maximum_group_size)),
                Value::Boolean(battleground.can_enter),
                Value::Boolean(battleground.holiday),
                Value::Boolean(battleground.random),
            ]))
        })?,
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
