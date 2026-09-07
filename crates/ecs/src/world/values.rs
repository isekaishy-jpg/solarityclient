//! Replicated world-state fields, separate from per-object update fields.

use std::collections::HashMap;

/// Session-owned server fields queried by WorldStateZoneSounds and world UI.
#[derive(Debug, Default)]
pub struct WorldStateValues {
    location: [u32; 3],
    values: HashMap<u32, u32>,
}

impl WorldStateValues {
    /// Returns 548D10's zero for an absent field.
    pub fn value(&self, field: u32) -> u32 {
        self.values.get(&field).copied().unwrap_or(0)
    }

    /// Returns the last initialization's map, zone, and area.
    pub const fn location(&self) -> [u32; 3] {
        self.location
    }

    /// Applies 549440's exact replacement and reports a changed visible value.
    pub fn set(&mut self, field: u32, value: u32) -> bool {
        self.values.insert(field, value).unwrap_or(0) != value
    }

    /// Applies 548970 and ordered 549440 calls. Native initialization updates
    /// the UI location filter but does not erase omitted world-state fields.
    pub fn initialize(&mut self, location: [u32; 3], values: &[(u32, u32)]) {
        self.location = location;
        for &(field, value) in values {
            self.set(field, value);
        }
    }
}
