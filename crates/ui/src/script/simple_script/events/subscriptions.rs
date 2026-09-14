//! A live ordered index preserves mutations made during a callback traversal.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Included};

use mlua::Lua;

/// Object IDs are monotonic Lua arena slots, matching the original scan order.
#[derive(Default)]
struct Subscriptions {
    events: BTreeMap<&'static str, BTreeSet<usize>>,
    all: BTreeSet<usize>,
}

/// Attaches subscription ownership before any construction callback can register.
pub(in super::super) fn initialize(lua: &Lua) {
    lua.set_app_data(Subscriptions::default());
}

/// Duplicate registration retains the same single delivery position.
pub(super) fn register(lua: &Lua, event: &'static str, index: usize) -> mlua::Result<()> {
    lua.app_data_mut::<Subscriptions>()
        .ok_or_else(|| mlua::Error::runtime("event subscriptions are not initialized"))?
        .events
        .entry(event)
        .or_default()
        .insert(index);
    Ok(())
}

/// Explicit registration is independent of RegisterAllEvents, like the Lua owner.
pub(super) fn unregister(lua: &Lua, event: &'static str, index: usize) -> mlua::Result<()> {
    let mut subscriptions = lua
        .app_data_mut::<Subscriptions>()
        .ok_or_else(|| mlua::Error::runtime("event subscriptions are not initialized"))?;
    if let Some(indices) = subscriptions.events.get_mut(event) {
        indices.remove(&index);
        if indices.is_empty() {
            subscriptions.events.remove(event);
        }
    }
    Ok(())
}

/// All-event admission joins the explicit list without duplicate deliveries.
pub(super) fn register_all(lua: &Lua, index: usize) -> mlua::Result<()> {
    lua.app_data_mut::<Subscriptions>()
        .ok_or_else(|| mlua::Error::runtime("event subscriptions are not initialized"))?
        .all
        .insert(index);
    Ok(())
}

/// Retires the owner's complete membership, including empty event buckets.
pub(super) fn unregister_all(lua: &Lua, index: usize) -> mlua::Result<()> {
    let mut subscriptions = lua
        .app_data_mut::<Subscriptions>()
        .ok_or_else(|| mlua::Error::runtime("event subscriptions are not initialized"))?;
    subscriptions.all.remove(&index);
    subscriptions.events.retain(|_, indices| {
        indices.remove(&index);
        !indices.is_empty()
    });
    Ok(())
}

/// Requeries after each handler so later registrations/removals are observed.
/// The entry-time object limit excludes newly created objects exactly as before.
/// No app-data borrow survives across a Lua callback or nested event dispatch.
pub(super) fn next(
    lua: &Lua,
    event: &str,
    after: usize,
    limit: usize,
) -> mlua::Result<Option<usize>> {
    if after >= limit {
        return Ok(None);
    }
    let subscriptions = lua
        .app_data_ref::<Subscriptions>()
        .ok_or_else(|| mlua::Error::runtime("event subscriptions are not initialized"))?;
    let range = (Excluded(after), Included(limit));
    let explicit = subscriptions
        .events
        .get(event)
        .and_then(|indices| indices.range(range).next().copied());
    let all = subscriptions.all.range(range).next().copied();
    Ok(match (explicit, all) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    })
}
