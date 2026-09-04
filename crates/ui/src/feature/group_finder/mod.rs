//! Dungeon and raid finder lifecycle projected from world-session state.

mod state;

use mlua::{Lua, MultiValue, Table, Value};

use crate::UiWorldState;

pub use state::{
    UiGroupFinderError, UiGroupFinderProposal, UiGroupFinderRole, UiGroupFinderRoleCheck,
    UiGroupFinderServerInfo, UiGroupFinderState,
};

/// Registers the stock queries consumed by `GetLFGMode`.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiGroupFinderState,
    world: UiWorldState,
) -> mlua::Result<()> {
    let proposal_state = state.clone();
    let server_state = state.clone();
    let role_state = state.clone();
    let listed_state = state.clone();
    let party_state = state.clone();
    let dungeon_state = state.clone();
    let restriction_state = state.clone();
    globals.raw_set(
        "GetLFGProposal",
        lua.create_function(move |lua, ()| proposal_values(lua, proposal_state.proposal()))?,
    )?;
    globals.raw_set(
        "GetLFGInfoServer",
        lua.create_function(move |lua, ()| server_values(lua, server_state.server_info()))?,
    )?;
    globals.raw_set(
        "GetLFGRoleUpdate",
        lua.create_function(move |_, ()| {
            let role = role_state.role_check();
            if !role.in_progress() {
                return Ok(MultiValue::from_vec(vec![Value::Boolean(false)]));
            }
            Ok(MultiValue::from_vec(vec![
                Value::Boolean(true),
                Value::Integer(i64::from(role.slot_count())),
                Value::Integer(i64::from(role.member_count())),
            ]))
        })?,
    )?;
    globals.raw_set(
        "IsListedInLFR",
        lua.create_function(move |_, ()| Ok(listed_state.is_listed_in_lfr()))?,
    )?;
    globals.raw_set(
        "IsPartyLFG",
        lua.create_function(move |_, ()| Ok(party_state.is_party_lfg()))?,
    )?;
    globals.raw_set(
        "IsInLFGDungeon",
        lua.create_function(move |_, ()| Ok(dungeon_state.is_in_lfg_dungeon()))?,
    )?;
    globals.raw_set(
        "HasLFGRestrictions",
        lua.create_function(move |_, ()| Ok(restriction_state.has_restrictions()))?,
    )?;
    globals.raw_set(
        "GetLFGQueuedList",
        lua.create_function(|_, output: Table| {
            // Script_GetLFGQueuedList at 0x00557520 clears the caller-owned
            // output table before appending each active queued dungeon. The
            // initial world projection owns no queued dungeon entries.
            output.clear()?;
            Ok(output)
        })?,
    )?;
    globals.raw_set(
        "GetLFGRoles",
        lua.create_function(|_, ()| {
            // Script_GetLFGRoles at 0x00552E10 returns the four role-mask
            // bits in leader, tank, healer, and damage order. A newly
            // entered world owns an empty mask until the player selects one.
            Ok((false, false, false, false))
        })?,
    )?;
    globals.raw_set(
        "GetAvailableRoles",
        lua.create_function(move |_, ()| {
            // Script_GetAvailableRoles at 0x005548F0 indexes the stock class
            // role mask, then returns tank, healer, and damage bits.
            let roles =
                world
                    .player_class()
                    .map_or((false, false, false), |class| match class.id() {
                        1 | 6 => (true, false, true),
                        2 | 11 => (true, true, true),
                        5 | 7 => (false, true, true),
                        3 | 4 | 8 | 9 => (false, false, true),
                        _ => (false, false, false),
                    });
            Ok(roles)
        })?,
    )?;
    globals.raw_set(
        "CanPartyLFGBackfill",
        lua.create_function(|_, ()| {
            // Script_CanPartyLFGBackfill at 0x00553170 only returns true for
            // an eligible undersized LFG party. The inactive queue is false.
            Ok(false)
        })?,
    )?;
    globals.raw_set(
        "GetLFGDeserterExpiration",
        lua.create_function(|_, ()| {
            // Script_GetLFGDeserterExpiration at 0x005580E0 returns no Lua
            // values unless an active deserter aura can be resolved.
            Ok(None::<f64>)
        })?,
    )?;
    globals.raw_set(
        "GetLFGRandomCooldownExpiration",
        lua.create_function(|_, ()| {
            // Script_GetLFGRandomCooldownExpiration at 0x00558060 likewise
            // produces no value until the corresponding aura is present.
            Ok(None::<f64>)
        })?,
    )
}

fn proposal_values(lua: &Lua, proposal: Option<UiGroupFinderProposal>) -> mlua::Result<MultiValue> {
    let Some(proposal) = proposal else {
        return Ok(MultiValue::from_vec(vec![Value::Boolean(false)]));
    };
    Ok(MultiValue::from_vec(vec![
        Value::Boolean(true),
        Value::Integer(i64::from(proposal.type_id())),
        Value::Integer(i64::from(proposal.dungeon_id())),
        Value::String(lua.create_string(proposal.name())?),
        Value::String(lua.create_string(proposal.texture())?),
        Value::String(lua.create_string(proposal.role().as_str())?),
        Value::Boolean(proposal.has_responded()),
        Value::Integer(i64::from(proposal.total_encounters())),
        Value::Integer(i64::from(proposal.completed_encounters())),
        Value::Integer(i64::from(proposal.member_count())),
        Value::Boolean(proposal.is_leader()),
        Value::Boolean(proposal.is_holiday()),
    ]))
}

fn server_values(lua: &Lua, server: UiGroupFinderServerInfo) -> mlua::Result<MultiValue> {
    Ok(MultiValue::from_vec(vec![
        Value::Boolean(server.in_party()),
        Value::Boolean(server.joined()),
        Value::Boolean(server.queued()),
        Value::Boolean(server.no_partial_clear()),
        Value::Integer(i64::from(server.achievement_count())),
        Value::String(lua.create_string(server.comment())?),
        Value::Integer(i64::from(server.slot_count())),
    ]))
}
