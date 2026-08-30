//! Stock implementation responsibility recovered from `Object_C.cpp`.

use shipyard::Component;

/// Stable server GUID attached to every network-created world object.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Component)]
pub struct ObjectGuid(u64);

impl ObjectGuid {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the exact 64-bit world object GUID.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}
