//! Dungeon and raid finder lifecycle projected from world-session state.

mod state;

use mlua::{Lua, MultiValue, Table, Value};

pub use state::{
    UiGroupFinderError, UiGroupFinderProposal, UiGroupFinderRole, UiGroupFinderRoleCheck,
    UiGroupFinderServerInfo, UiGroupFinderState,
};

/// Registers the stock queries consumed by `GetLFGMode`.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiGroupFinderState,
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
