//! Unit-owned water transitions, independent of movement animation callbacks.

use glam::Vec3;
use solarity_ecs::WorldObjectIdentity;

/// Frozen 730D10 splash notification at the registered unit position.
#[derive(Clone, Copy, Debug)]
pub(super) struct UnitWaterSplash {
    pub identity: WorldObjectIdentity,
    pub position: Vec3,
}
