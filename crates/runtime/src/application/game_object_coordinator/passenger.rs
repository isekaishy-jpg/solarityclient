//! GameObject virtual passenger admission and map-handle retention predicates.

use glam::Vec3;
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_systems::{MovementCollectionError, MovementCollisionBounds, MovementTransportVolume};

use super::{GameObjectResource, RuntimeGameObjectPresentation};

impl RuntimeGameObjectPresentation {
    /// Resolves 70FFD0's still-live parent before refresh retires its generation.
    /// Other behaviors do not own the transport passenger destruction callback.
    pub(in crate::application) fn retiring_passenger_frame(
        &self,
        world: &ActiveWorld,
        identity: WorldObjectIdentity,
    ) -> Result<
        Option<solarity_systems::MovementTransportFrame>,
        solarity_systems::GameObjectPlacementError,
    > {
        if world.object_identity(identity.guid()) == Some(identity) {
            return Ok(None);
        }
        let Some(instance) = self.movement_instance(identity) else {
            return Ok(None);
        };
        if !matches!(instance.presentation.object_type(), 11 | 15) {
            return Ok(None);
        }
        self.object_movement_frame(identity)
    }

    /// Returns the MO parent's last published passenger phase (`virtual +0xA8`).
    /// The clock is zero before its first route sample and survives a sample
    /// on another map. The active mover owns publication into movement packets.
    #[must_use]
    pub fn object_passenger_time_ms(&self, identity: WorldObjectIdentity) -> Option<u32> {
        Some(
            self.movement_instance(identity)?
                .transport
                .as_ref()?
                .passenger_time_ms(),
        )
    }

    /// Tests the exact lifetime's GAMEOBJECT_FLAGS bit 8 (`0x00712F20`).
    /// Only the active mover's contact owner may use this boarding predicate.
    #[must_use]
    pub fn object_can_board(&self, identity: WorldObjectIdentity) -> bool {
        self.movement_instance(identity)
            .is_some_and(|instance| instance.presentation.flags() & 8 != 0)
    }

    /// Tests the transport behavior's retention volume in parent coordinates.
    /// Ordinary GameObject behaviors return false; type 11 and 15 delegate to
    /// `0x0070B360`. A missing map handle retains an existing passenger.
    ///
    /// # Errors
    /// Reports an invalid authored M2 collision box.
    pub fn object_retains_passenger(
        &self,
        identity: WorldObjectIdentity,
        position: Vec3,
    ) -> Result<bool, MovementCollectionError> {
        let Some(instance) = self.movement_instance(identity) else {
            return Ok(false);
        };
        if !matches!(instance.presentation.object_type(), 11 | 15) {
            return Ok(false);
        }
        if instance.map_placement().is_none() {
            return Ok(true);
        }
        let volume = match instance.resource() {
            None => MovementTransportVolume::Unbounded,
            Some(GameObjectResource::M2(source)) => {
                // Resource publication occurs after full CPU decode, which is
                // the native ready gate. Header bounds are independent of the
                // visible mesh and of whether collision triangles are empty.
                let bounds = source.model().collision_bounds();
                MovementTransportVolume::Model(Some(MovementCollisionBounds::new(
                    bounds.minimum(),
                    bounds.maximum(),
                )?))
            }
            Some(GameObjectResource::WorldModel(source)) => MovementTransportVolume::WorldModel {
                loaded_groups: source.model().groups().len(),
                planes: source.model().convex_volume_planes(),
            },
        };
        Ok(volume.contains(position))
    }
}
