//! Unit-owned water transitions, independent of movement animation callbacks.

use glam::Vec3;
use solarity_ecs::{WorldMovementState, WorldObjectIdentity, WorldTransform};

/// A completed movement registration, shared with the scene ripple emitter.
#[derive(Clone, Copy, Debug)]
pub(super) struct UnitWaterSample {
    pub identity: WorldObjectIdentity,
    pub transform: WorldTransform,
    pub movement: WorldMovementState,
    pub liquid: Option<solarity_systems::SubmergedLiquid>,
    pub height: f32,
    pub splash: bool,
}

/// Frozen 730D10 splash notification at the registered unit position.
#[derive(Clone, Copy, Debug)]
pub(super) struct UnitWaterSplash {
    pub identity: WorldObjectIdentity,
    pub position: Vec3,
}
