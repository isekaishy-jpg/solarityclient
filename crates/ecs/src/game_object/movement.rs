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
    /// Native GameObject+0x200 difference from the local wrapping client clock.
    transport_clock_offset_ms: u32,
}

impl GameObjectMovement {
    /// Creates the complete admitted GameObject movement snapshot.
    #[must_use]
    pub const fn new(packed_rotation: u64, transport: Option<GameObjectTransport>) -> Self {
        Self {
            packed_rotation,
            transport,
            transport_clock_offset_ms: 0,
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

    /// Anchors the received path clock at object creation (native 714250).
    #[must_use]
    pub const fn with_transport_clock(mut self, progress_ms: u32, receipt_ms: u32) -> Self {
        self.transport_clock_offset_ms = progress_ms.wrapping_sub(receipt_ms);
        self
    }

    /// Returns the running transport path clock used by 7134A0/711F20.
    #[must_use]
    pub const fn transport_clock_ms(self, client_time_ms: u32) -> u32 {
        client_time_ms.wrapping_add(self.transport_clock_offset_ms)
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
