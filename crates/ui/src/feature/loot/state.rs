//! Active loot-window master-loot eligibility projection.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, Table, Value};

/// Shared ordered names eligible for the active master-loot item.
#[derive(Clone, Debug, Default)]
pub struct UiLootState {
    master_loot_candidates: Rc<RefCell<Vec<Option<String>>>>,
}

impl UiLootState {
    /// Creates the closed loot-window image with no eligible candidates.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces candidate slots in party or raid roster order.
    pub fn set_master_loot_candidates(&self, candidates: Vec<Option<String>>) {
        *self.master_loot_candidates.borrow_mut() = candidates;
    }

    /// Returns one one-based eligible candidate name.
    #[must_use]
    pub fn master_loot_candidate(&self, index: usize) -> Option<String> {
        index
            .checked_sub(1)
            .and_then(|index| self.master_loot_candidates.borrow().get(index).cloned())
            .flatten()
    }
}

/// Registers active-loot queries consumed while constructing the dropdown.
pub(crate) fn register_globals(lua: &Lua, globals: &Table, state: UiLootState) -> mlua::Result<()> {
    globals.raw_set(
        "GetMasterLootCandidate",
        lua.create_function(move |lua, value: Value| {
            let number = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: GetMasterLootCandidate(index)"))?;
            let index = if number.is_finite() && number >= 1.0 && number <= usize::MAX as f64 {
                number.round() as usize
            } else {
                0
            };
            state
                .master_loot_candidate(index)
                .map(|name| lua.create_string(name))
                .transpose()
        })?,
    )
}
