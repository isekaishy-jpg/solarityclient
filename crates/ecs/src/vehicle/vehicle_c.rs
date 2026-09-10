//! UnitVehicle_C ownership retained independently of movement snapshots.

use shipyard::Component;

/// Native unit +F5C owner, present even when Vehicle.dbc has no matching row.
#[derive(Clone, Copy, Debug, PartialEq, Component)]
pub struct UnitVehicle {
    definition_id: u32,
    initial_facing: f32,
    initial_transform: Option<crate::WorldTransform>,
}

impl UnitVehicle {
    pub(crate) const fn new(
        definition_id: u32,
        initial_facing: f32,
        initial_transform: Option<crate::WorldTransform>,
    ) -> Self {
        Self {
            definition_id,
            initial_facing,
            initial_transform,
        }
    }

    /// Returns the most recently admitted Vehicle.dbc identifier.
    #[must_use]
    pub const fn definition_id(self) -> u32 {
        self.definition_id
    }

    /// Returns the facing captured on owner creation; definition changes retain it.
    #[must_use]
    pub const fn initial_facing(self) -> f32 {
        self.initial_facing
    }

    /// Returns the creation world pose used to seed Vehicle_C's cached matrix.
    /// Later snapshots and definition changes cannot replace this seed.
    #[must_use]
    pub const fn initial_transform(self) -> Option<crate::WorldTransform> {
        self.initial_transform
    }
}
