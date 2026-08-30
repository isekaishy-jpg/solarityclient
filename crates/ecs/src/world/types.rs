//! RTTI-backed type, state, and identifier vocabulary for this stock responsibility.

use glam::Vec3;

/// Numeric Map.dbc identifier for the active world.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldMapId(u32);

impl WorldMapId {
    /// Creates a map identifier from the exact server value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the exact Map.dbc identifier.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Authoritative facts required to create the initial local-player entity.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldBootstrap {
    map_id: WorldMapId,
    player_guid: u64,
    player_name: String,
    position: Vec3,
    orientation: f32,
}

impl WorldBootstrap {
    /// Creates an ECS bootstrap from a validated world-login result.
    #[must_use]
    pub fn new(
        map_id: WorldMapId,
        player_guid: u64,
        player_name: impl Into<String>,
        position: Vec3,
        orientation: f32,
    ) -> Self {
        Self {
            map_id,
            player_guid,
            player_name: player_name.into(),
            position,
            orientation,
        }
    }

    pub(crate) fn into_parts(self) -> (WorldMapId, u64, String, Vec3, f32) {
        (
            self.map_id,
            self.player_guid,
            self.player_name,
            self.position,
            self.orientation,
        )
    }
}
