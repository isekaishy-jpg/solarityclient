//! Build-12340 UnitCombatLog_C script wrappers (0x0074D580–0x00751120).

use mlua::{Lua, MultiValue, Table, Value};

use crate::world::combat_log::{
    CombatLogFilter, CombatLogObjectFilter, complete_object_mask, parse_guid,
};
use crate::{UiEventArgument, UiScriptEnvironment};

pub(super) fn register(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let state = environment.combat_log_state();
    globals.raw_set(
        "CombatLogResetFilter",
        lua.create_function(move |_, ()| {
            state.reset_filter();
            Ok(())
        })?,
    )?;
    let state = environment.combat_log_state();
    globals.raw_set(
        "CombatLogAddFilter",
        lua.create_function(
            move |lua, (events, source, destination, spell): (Value, Value, Value, Value)| {
                let events = lua
                    .coerce_string(events)?
                    .map(|text| text.to_str().map(|text| text.to_owned()))
                    .transpose()?;
                let source = object_filter(source, "source")?;
                let destination = object_filter(destination, "destination")?;
                let (spell_id, spell_name) = match spell {
                    Value::Integer(id) => (id as u32, None),
                    Value::Number(id) => (id as i64 as u32, None),
                    Value::String(name) => (0, Some(name.to_str()?.to_owned())),
                    _ => (0, None),
                };
                state.add_filter(CombatLogFilter {
                    events: CombatLogFilter::events(events.as_deref()),
                    source,
                    destination,
                    spell_id,
                    spell_name,
                });
                Ok(())
            },
        )?,
    )?;
    let state = environment.combat_log_state();
    globals.raw_set(
        "CombatLogGetNumEntries",
        lua.create_function(move |_, ignore: Value| Ok(state.count(native_bool(&ignore))))?,
    )?;
    let state = environment.combat_log_state();
    globals.raw_set(
        "CombatLogSetCurrentEntry",
        lua.create_function(move |lua, (index, ignore): (Value, Value)| {
            let index = required_integer(
                lua,
                index,
                "Usage: CombatLogSetCurrentEntry(index [, ignoreFilter])",
            )?;
            Ok(state.set_current(index, native_bool(&ignore)).then_some(1))
        })?,
    )?;
    let state = environment.combat_log_state();
    globals.raw_set(
        "CombatLogAdvanceEntry",
        lua.create_function(move |lua, (count, ignore): (Value, Value)| {
            let count = required_integer(
                lua,
                count,
                "Usage: CombatLogAdvanceEntry(count [, ignoreFilter])",
            )?;
            Ok(state.advance(count, native_bool(&ignore)).then_some(1))
        })?,
    )?;
    let state = environment.combat_log_state();
    globals.raw_set(
        "CombatLogGetCurrentEntry",
        lua.create_function(move |lua, ()| {
            let Some(payload) = state.current_payload() else {
                return Ok(MultiValue::new());
            };
            let values = payload
                .arguments()
                .iter()
                .map(|argument| match argument {
                    UiEventArgument::Nil => Ok(Value::Nil),
                    UiEventArgument::Boolean(value) => Ok(Value::Boolean(*value)),
                    UiEventArgument::Integer(value) => Ok(Value::Integer(*value)),
                    UiEventArgument::Number(value) => Ok(Value::Number(*value)),
                    UiEventArgument::String(value) => lua.create_string(value).map(Value::String),
                })
                .collect::<mlua::Result<Vec<_>>>()?;
            Ok(MultiValue::from_vec(values))
        })?,
    )?;
    let state = environment.combat_log_state();
    globals.raw_set(
        "CombatLogClearEntries",
        lua.create_function(move |_, ()| {
            state.clear();
            Ok(())
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "CombatLogGetRetentionTime",
        lua.create_function(move |_, ()| {
            Ok(cvars.number("combatLogRetentionTime").unwrap_or(300.0) as i32)
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "CombatLogSetRetentionTime",
        lua.create_function(move |lua, seconds: Value| {
            let seconds =
                required_integer(lua, seconds, "Usage: CombatLogSetRetentionTime(seconds)")?;
            cvars
                .set("combatLogRetentionTime", seconds.to_string())
                .map_err(|error| mlua::Error::runtime(format!("{error:?}")))
        })?,
    )?;
    globals.raw_set(
        "CombatLog_Object_IsA",
        lua.create_function(|lua, (flags, mask): (Value, Value)| {
            let flags = lua.coerce_number(flags)?.unwrap_or(0.0) as i64 as u32;
            let mask = lua.coerce_number(mask)?.unwrap_or(0.0) as i64 as u32;
            Ok(complete_object_mask(flags & mask).then_some(1))
        })?,
    )?;
    let state = environment.combat_log_state();
    let world = environment.world_state();
    globals.raw_set(
        "CombatTextSetActiveUnit",
        lua.create_function(move |lua, unit: Value| {
            let unit = lua.coerce_string(unit)?;
            let guid = match unit {
                Some(unit) if unit.to_str()?.eq_ignore_ascii_case("player") => world.player_guid(),
                _ => None,
            };
            state.set_active_text_unit(guid);
            Ok(())
        })?,
    )?;
    Ok(())
}

fn object_filter(value: Value, label: &str) -> mlua::Result<CombatLogObjectFilter> {
    let mask = match value {
        Value::Integer(mask) => mask as u32,
        Value::Number(mask) => mask as i64 as u32,
        Value::String(guid) => return Ok(CombatLogObjectFilter::Guid(parse_guid(&guid.to_str()?))),
        _ => return Ok(CombatLogObjectFilter::Mask(u32::MAX)),
    };
    if !complete_object_mask(mask) {
        return Err(mlua::Error::runtime(format!(
            "CombatLogAddFilter: incomplete {label} object filter"
        )));
    }
    Ok(CombatLogObjectFilter::Mask(mask))
}

fn required_integer(lua: &Lua, value: Value, usage: &str) -> mlua::Result<i32> {
    lua.coerce_number(value)?
        .map(|number| number as i32)
        .ok_or_else(|| mlua::Error::runtime(usage))
}

fn native_bool(value: &Value) -> bool {
    super::super::native_optional_bool(Some(value), false)
}
