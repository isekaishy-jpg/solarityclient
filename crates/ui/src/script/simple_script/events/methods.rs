//! Lua subscription methods publish their native membership immediately.

use super::super::{
    UiManifestKind, all_events_key, event_table, events_key, index_key, registered_event,
};
use super::subscriptions;
use mlua::{Lua, Table, Value};

/// Installs stock registration and query methods with canonical event names.
pub(in super::super) fn register_frame_event_methods(
    lua: &Lua,
    methods: &Table,
    manifest_kind: UiManifestKind,
) -> mlua::Result<()> {
    methods.raw_set(
        "RegisterEvent",
        lua.create_function(move |lua, (object, name): (Table, String)| {
            let canonical = registered_event(manifest_kind, &name)?;
            if let Some(canonical) = canonical {
                event_table(&object)?.raw_set(canonical, true)?;
                subscriptions::register(lua, canonical, object.raw_get(index_key())?)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "UnregisterEvent",
        lua.create_function(move |lua, (object, name): (Table, String)| {
            let canonical = registered_event(manifest_kind, &name)?;
            if let Some(canonical) = canonical {
                event_table(&object)?.raw_set(canonical, Value::Nil)?;
                subscriptions::unregister(lua, canonical, object.raw_get(index_key())?)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "RegisterAllEvents",
        lua.create_function(|lua, object: Table| {
            object.raw_set(all_events_key(), true)?;
            subscriptions::register_all(lua, object.raw_get(index_key())?)
        })?,
    )?;
    methods.raw_set(
        "UnregisterAllEvents",
        lua.create_function(|lua, object: Table| {
            object.raw_set(events_key(), lua.create_table()?)?;
            object.raw_set(all_events_key(), false)?;
            subscriptions::unregister_all(lua, object.raw_get(index_key())?)
        })?,
    )?;
    methods.raw_set(
        "IsEventRegistered",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let Some(canonical) = registered_event(manifest_kind, &name)? else {
                return Ok(None::<bool>);
            };
            if object.raw_get::<bool>(all_events_key())? {
                return Ok(Some(true));
            }
            event_table(&object)?.raw_get::<Option<bool>>(canonical)
        })?,
    )?;
    Ok(())
}
