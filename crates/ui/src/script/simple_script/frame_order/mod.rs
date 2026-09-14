//! Native frame raising and subtree level propagation with targeted publication.

mod index;

pub(super) use index::{initialize, register, update};

use mlua::{Lua, Table};

use super::{
    DIRTY_FRAME_ORDER, OBJECT_CHILDREN_REGISTRY, OBJECT_REGISTRY, frame_level_key,
    frame_strata_key, frame_top_level_key, index_key, mark_object_state_changed, parent_key,
};

/// Raises the stock top-level owner and preserves its descendants' level offsets.
pub(super) fn raise(lua: &Lua, object: &Table) -> mlua::Result<()> {
    let _profile = solarity_profiling::profile!("ui.frame.raise");
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut raised = object.clone();
    loop {
        if raised.raw_get::<bool>(frame_top_level_key())? {
            break;
        }
        let Some(parent_index) = raised.raw_get::<Option<usize>>(parent_key())? else {
            return Ok(());
        };
        let Some(parent) = objects.raw_get::<Option<Table>>(parent_index)? else {
            return Ok(());
        };
        raised = parent;
    }

    let strata = raised.raw_get::<String>(frame_strata_key())?;
    let top_level = index::next_level(lua, &strata)?;
    set_level(lua, &raised, top_level)
}

/// Applies stock's bounded level delta while retaining every child's offset.
/// Journal each changed frame so packet and hit-test order can be republished
/// from this subtree without snapshotting every unrelated Lua object.
pub(super) fn set_level(lua: &Lua, object: &Table, requested_level: i32) -> mlua::Result<()> {
    let old_level = object.raw_get::<i32>(frame_level_key())?;
    let delta = requested_level.max(0).saturating_sub(old_level).min(128);
    if delta == 0 {
        return Ok(());
    }

    object.raw_set(frame_level_key(), old_level.saturating_add(delta))?;
    index::update(lua, object)?;
    mark_object_state_changed(lua, object, DIRTY_FRAME_ORDER)?;

    let root_index = object.raw_get::<usize>(index_key())?;
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let children: Table = lua.named_registry_value(OBJECT_CHILDREN_REGISTRY)?;
    let mut pending = Vec::new();
    if let Some(direct) = children.raw_get::<Option<Table>>(root_index)? {
        for child in direct.sequence_values::<usize>() {
            pending.push(child?);
        }
    }
    while let Some(index) = pending.pop() {
        let candidate: Table = objects.raw_get(index)?;
        if let Some(level) = candidate.raw_get::<Option<i32>>(frame_level_key())? {
            candidate.raw_set(frame_level_key(), level.saturating_add(delta).max(0))?;
            index::update(lua, &candidate)?;
            mark_object_state_changed(lua, &candidate, DIRTY_FRAME_ORDER)?;
        }
        if let Some(direct) = children.raw_get::<Option<Table>>(index)? {
            for child in direct.sequence_values::<usize>() {
                pending.push(child?);
            }
        }
    }
    Ok(())
}
