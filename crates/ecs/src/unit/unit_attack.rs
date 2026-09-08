//! Server-owned attack GUID, distinct from the replicated target field.

use shipyard::Component;

/// Unit_C +A20, set by attack start and cleared by stop/death.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct UnitAttackTarget(u64);

impl UnitAttackTarget {
    /// Retains the full GUID; zero ends the attack.
    #[must_use]
    pub const fn new(guid: u64) -> Self {
        Self(guid)
    }

    /// Returns the current attack GUID without requiring target residency.
    #[must_use]
    pub const fn guid(self) -> u64 {
        self.0
    }
}
