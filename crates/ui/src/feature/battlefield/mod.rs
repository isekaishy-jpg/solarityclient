//! Battlefield queue state projected from build-12340 world-session packets.

mod state;

use mlua::{Lua, Table};

pub use state::{
    MAX_BATTLEFIELD_QUEUES, MAX_WORLD_PVP_QUEUES, UiBattlefieldQueueError, UiBattlefieldQueueState,
    UiBattlefieldQueueStatus, UiBattlefieldSlot, UiWorldPvpQueueSlot,
};

/// Registers the seven-value build-12340 battlefield status query.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiBattlefieldQueueState,
) -> mlua::Result<()> {
    let battlefield = state.clone();
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
    )
}
