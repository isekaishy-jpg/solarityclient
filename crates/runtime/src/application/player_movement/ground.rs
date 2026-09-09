//! Retained collision normals published independently of authoritative positions.

#[cfg(test)]
#[path = "../../../tests/application/movement_ground_normal.rs"]
mod tests;

use glam::Vec3;
use solarity_systems::MovementCollisionVolume;

use super::{
    LocalMovement, LocalMovementGeometry, RuntimePlayerMovement, RuntimePlayerMovementError,
};

impl RuntimePlayerMovement {
    /// Reports the current owner's world-space normal with its lifetime identity.
    pub(in crate::application) fn ground_sample(
        &self,
    ) -> Option<(solarity_ecs::WorldObjectIdentity, Vec3)> {
        self.owner
            .as_ref()
            .map(|owner| (owner.identity, owner.world_ground_normal()))
    }
}

impl LocalMovement {
    /// 762E00's common exit invokes 75EE60, including landing/parent-change exits.
    /// Candidate collection remains owned by the movement interval that just ran.
    pub(super) fn update_ground_normal<G: LocalMovementGeometry>(
        &mut self,
        dimensions: [f32; 3],
        geometry: &G,
    ) -> Result<(), RuntimePlayerMovementError> {
        let [radius, height, _] = dimensions;
        let volume = MovementCollisionVolume::new(self.position, radius, height)
            .map_err(super::super::RuntimeStaticMovementError::from)?;
        self.ground_normal = volume.ground_normal(geometry.triangles());
        Ok(())
    }

    /// 987490 rotates the stored normal through the current passenger frame.
    pub(super) fn world_ground_normal(&self) -> Vec3 {
        self.passenger.map_or(self.ground_normal, |parent| {
            parent.frame.world_direction(self.ground_normal)
        })
    }
}
