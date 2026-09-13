//! Highest frame levels maintained at the existing registration/mutation boundary.

use std::collections::{BTreeMap, HashMap};

use mlua::{Lua, Table};

use super::super::{frame_level_key, frame_strata_key, index_key};

/// Exact Lua order fields, independent of visibility and top-level ownership.
#[derive(Eq, PartialEq)]
struct FrameOrder {
    strata: String,
    level: i32,
}

/// Counts retain duplicate levels when another frame moves down or changes strata.
#[derive(Default)]
struct FrameOrderIndex {
    /// One-based OBJECT_REGISTRY indices; the registry retains their lifetimes.
    owners: HashMap<usize, FrameOrder>,
    strata: HashMap<String, BTreeMap<i32, usize>>,
}

impl FrameOrderIndex {
    /// Replaces one published owner without scanning any unrelated Lua tables.
    /// Both indexes change together; missing counts are an implementation defect,
    /// never an absent frame or malformed script input.
    #[allow(clippy::expect_used)]
    fn replace(&mut self, owner: usize, order: FrameOrder) {
        if self.owners.get(&owner) == Some(&order) {
            return;
        }
        *self
            .strata
            .entry(order.strata.clone())
            .or_default()
            .entry(order.level)
            .or_default() += 1;
        if let Some(previous) = self.owners.insert(owner, order) {
            let levels = self
                .strata
                .get_mut(&previous.strata)
                .expect("every indexed frame has a strata level count");
            let count = levels
                .get_mut(&previous.level)
                .expect("every indexed frame contributes to its level count");
            *count -= 1;
            if *count == 0 {
                levels.remove(&previous.level);
            }
        }
    }

    /// Mirrors Raise's original zero-based maximum and saturating successor.
    fn next_level(&self, strata: &str) -> i32 {
        self.strata
            .get(strata)
            .and_then(|levels| levels.last_key_value())
            .map_or(0, |(&level, _)| level.saturating_add(1).max(0))
    }
}

/// Reset with OBJECT_REGISTRY, including a second runtime in the same Lua state.
pub(in super::super) fn initialize(lua: &Lua) {
    lua.set_app_data(FrameOrderIndex::default());
}

/// Register only after the complete table joins OBJECT_REGISTRY. Regions without
/// frame order do not contribute, exactly as in the previous registry scan.
pub(in super::super) fn register(lua: &Lua, object: &Table) -> mlua::Result<()> {
    let Some(strata) = object.raw_get::<Option<String>>(frame_strata_key())? else {
        return Ok(());
    };
    let owner = object.raw_get(index_key())?;
    let level = object
        .raw_get::<Option<i32>>(frame_level_key())?
        .unwrap_or(0);
    lua.app_data_mut::<FrameOrderIndex>()
        .ok_or_else(|| mlua::Error::runtime("frame order index is unavailable"))?
        .replace(owner, FrameOrder { strata, level });
    Ok(())
}

/// Unpublished construction tables join only at registration; ordinary setters
/// replace their already published order before any later callback can Raise.
pub(in super::super) fn update(lua: &Lua, object: &Table) -> mlua::Result<()> {
    let owner = object.raw_get::<usize>(index_key())?;
    let published = lua
        .app_data_ref::<FrameOrderIndex>()
        .ok_or_else(|| mlua::Error::runtime("frame order index is unavailable"))?
        .owners
        .contains_key(&owner);
    if published {
        register(lua, object)?;
    }
    Ok(())
}

/// Borrows the current registry's order counts without retaining a Lua guard.
pub(super) fn next_level(lua: &Lua, strata: &str) -> mlua::Result<i32> {
    Ok(lua
        .app_data_ref::<FrameOrderIndex>()
        .ok_or_else(|| mlua::Error::runtime("frame order index is unavailable"))?
        .next_level(strata))
}
