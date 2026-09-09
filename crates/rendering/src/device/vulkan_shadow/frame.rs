//! Borrowed original primary shadow projection and admitted M2 draw packets.

use crate::{M2PreparedDraw, WorldShadowProjection};

/// Primary unit-shadow pass sharing the world frame's animated bone palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPrimaryShadowFrame<'a> {
    projection: WorldShadowProjection,
    casters: &'a [M2PreparedDraw],
}

impl<'a> WorldPrimaryShadowFrame<'a> {
    /// Joins an original projection with spatially admitted player/creature batches.
    /// Packets whose material cannot cast a shadow are ignored by the pass.
    #[must_use]
    pub const fn new(projection: WorldShadowProjection, casters: &'a [M2PreparedDraw]) -> Self {
        Self {
            projection,
            casters,
        }
    }

    /// Returns this frame's caster and receiver transforms.
    #[must_use]
    pub const fn projection(self) -> WorldShadowProjection {
        self.projection
    }

    /// Returns packets referring to the world frame's supplied bone transforms.
    #[must_use]
    pub const fn casters(self) -> &'a [M2PreparedDraw] {
        self.casters
    }
}
