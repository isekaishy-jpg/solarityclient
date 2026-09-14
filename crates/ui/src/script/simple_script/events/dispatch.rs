//! Native creation-order callbacks visit event members rather than every UI region.

use mlua::{Lua, Table, Value};

use super::super::{OBJECT_REGISTRY, UiScriptHandler, call_event_handler, object_script_function};
use super::subscriptions;

/// Membership and handler changes by earlier callbacks apply to later owners.
pub(in super::super) fn dispatch_subscribers(
    lua: &Lua,
    object_count: usize,
    event: &str,
    payload: &[Value],
) -> mlua::Result<usize> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut dispatched = 0;
    let mut previous = 0;
    let mut visited = 0;
    while let Some(index) = subscriptions::next(lua, event, previous, object_count)? {
        previous = index;
        visited += 1;
        let object: Table = objects.raw_get(index)?;
        let Some(function) = object_script_function(lua, &object, UiScriptHandler::Event)? else {
            continue;
        };
        call_event_handler(lua, &function, object, event, payload)?;
        dispatched += 1;
    }
    solarity_profiling::profile_value!("ui.event.visited_subscribers", visited);
    Ok(dispatched)
}
