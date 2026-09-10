//! UnitVehicle_C ownership retained independently of movement snapshots.

use shipyard::Component;

/// Native unit +F5C owner, present even when Vehicle.dbc has no matching row.
#[derive(Clone, Copy, Debug, PartialEq, Component)]
pub struct UnitVehicle {
    definition_id: u32,
    initial_pitch: f32,
}

impl UnitVehicle {
    pub(crate) const fn new(definition_id: u32, initial_pitch: f32) -> Self {
        Self {
            definition_id,
            initial_pitch,
        }
    }

    /// Returns the most recently admitted Vehicle.dbc identifier.
    #[must_use]
    pub const fn definition_id(self) -> u32 {
        self.definition_id
    }

    /// Returns the pitch captured on owner creation; definition changes retain it.
    #[must_use]
    pub const fn initial_pitch(self) -> f32 {
        self.initial_pitch
    }
}
