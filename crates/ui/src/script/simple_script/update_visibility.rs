//! Native visibility checks retained between hierarchy mutations during dispatch.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Lua, Table};

use super::{is_script_frame_table, object_is_visible};

#[derive(Clone, Default)]
struct VisibilityEpoch(Rc<Cell<u64>>);

/// Stores only frame eligibility; subscriptions and handlers remain live Lua reads.
pub(super) struct UpdateVisibilityCache {
    epoch: VisibilityEpoch,
    observed_epoch: u64,
    values: Vec<Option<bool>>,
}

impl UpdateVisibilityCache {
    pub(super) fn new(lua: &Lua) -> Self {
        let epoch = VisibilityEpoch::default();
        lua.set_app_data(epoch.clone());
        Self {
            epoch,
            observed_epoch: 0,
            values: Vec::new(),
        }
    }

    pub(super) fn is_visible(
        &mut self,
        lua: &Lua,
        objects: &Table,
        index: usize,
    ) -> mlua::Result<bool> {
        let epoch = self.epoch.0.get();
        if epoch != self.observed_epoch {
            self.values.fill(None);
            self.observed_epoch = epoch;
        }
        if index >= self.values.len() {
            self.values.resize(index + 1, None);
        }
        if let Some(visible) = self.values[index] {
            return Ok(visible);
        }
        let object: Table = objects.raw_get(index)?;
        let visible = is_script_frame_table(&object)? && object_is_visible(lua, object)?;
        self.values[index] = Some(visible);
        Ok(visible)
    }
}

/// Mutation-side invalidation occurs before callbacks can reenter selection.
pub(super) fn invalidate(lua: &Lua) {
    if let Some(epoch) = lua.app_data_ref::<VisibilityEpoch>() {
        epoch.0.set(epoch.0.get().wrapping_add(1));
    }
}
