//! Replicated GameObject passenger and packed-rotation ownership.

use glam::Vec3;
use shipyard::Component;

/// Non-living movement inputs retained independently of sparse update fields.
///
/// Stock initializes an omitted create quaternion to packed zero (identity).
/// The ordinary facing word does not replace this quaternion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Component)]
pub struct GameObjectMovement {
    packed_rotation: u64,
    transport: Option<GameObjectTransport>,
}

impl GameObjectMovement {
    /// Creates the complete admitted GameObject movement snapshot.
    #[must_use]
    pub const fn new(packed_rotation: u64, transport: Option<GameObjectTransport>) -> Self {
        Self {
            packed_rotation,
            transport,
        }
    }

    /// Returns the exact 22/21/21-bit local quaternion from the movement block.
    #[must_use]
    pub const fn packed_rotation(self) -> u64 {
        self.packed_rotation
    }

    /// Returns the nonzero parent GUID and its local placement offset.
    #[must_use]
    pub const fn transport(self) -> Option<GameObjectTransport> {
        self.transport
    }
}

/// A GameObject passenger's parent and local positional inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameObjectTransport {
    /// Exact GUID of the movement parent.
    pub guid: u64,
    /// Position in the parent's coordinate system.
    pub position: Vec3,
    /// Final facing word from `UPDATEFLAG_POSITION`.
    pub orientation: f32,
}
