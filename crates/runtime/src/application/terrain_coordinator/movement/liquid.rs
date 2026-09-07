//! Camera liquid selection over the retained terrain and placed WMO owners.

use super::{MovementRootReference, RuntimeMovementRegistrationError, RuntimeTerrainCoordinator};
use glam::Vec3;
use solarity_asset::LiquidTypeCatalog;
use solarity_systems::{
    SubmergedLiquid, TerrainRegistrationPoint, WorldModelCameraRegistrationQuery,
    WorldModelRegistrationKind,
};

impl RuntimeTerrainCoordinator {
    /// Resolves 795D40's camera interior, then 790920's selected water provider.
    /// Queries only resident generations; no archive reads occur in this path.
    ///
    /// # Errors
    /// Returns malformed resident geometry or missing liquid behavior data.
    pub fn camera_submerged_liquid(
        &mut self,
        position: Vec3,
        liquids: &LiquidTypeCatalog,
    ) -> Result<Option<SubmergedLiquid>, RuntimeMovementRegistrationError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        let point = TerrainRegistrationPoint::new(position.x, position.y)?;
        let end = position - Vec3::Z * 1760.0;
        let maximum = active
            .tile_at(point.tile())
            .map(|tile| tile.collision.trace(position, end, 0.0, 1.0))
            .transpose()?
            .flatten()
            .map_or(1.0, |hit| hit.fraction());
        let mut camera = WorldModelCameraRegistrationQuery::new(position, end, maximum)?;
        for index in 0..active.movement.roots.len() {
            let root = active.movement.roots[index];
            camera.probe_root(
                index,
                match root {
                    MovementRootReference::Static(_) => WorldModelRegistrationKind::Static,
                    MovementRootReference::GameObject(_) => WorldModelRegistrationKind::Transformed,
                },
                active.registration_root_mut(root)?,
            )?;
        }
        if let Some(camera) = camera.finish() {
            let root = active.movement.roots[camera.owner];
            return Ok(active
                .registration_root_mut(root)?
                .registered_submerged_liquid(camera.group, position, liquids)?);
        }
        // General 7A0B00 tries each eligible WMO before falling back to ADT.
        for index in 0..active.movement.roots.len() {
            let root = active.movement.roots[index];
            if let Some(liquid) = active
                .registration_root_mut(root)?
                .submerged_liquid(position, liquids)?
            {
                return Ok(Some(liquid));
            }
        }
        let Some(tile) = active.tile_at(point.tile()) else {
            return Ok(None);
        };
        Ok(point.submerged_liquid(
            &tile.decoded,
            position.z,
            tile.collision.registration_height_at(point)?,
        )?)
    }
}
