//! Unit and camera liquid selection over retained terrain and placed WMO owners.

use super::{MovementRootReference, RuntimeMovementRegistrationError, RuntimeTerrainCoordinator};
use glam::Vec3;
use solarity_asset::LiquidTypeCatalog;
use solarity_systems::{
    SubmergedLiquid, TerrainRegistrationPoint, WorldModelCameraRegistrationQuery,
    WorldModelRegistrationKind,
};

impl RuntimeTerrainCoordinator {
    /// Appends 75FF90's water-only bank after ordinary collection has proved
    /// residency and synchronized the retained root generations.
    pub(super) fn collect_swimming_surfaces(
        &mut self,
        bounds: solarity_systems::MovementCollisionBounds,
        output: &mut Vec<solarity_systems::MovementCollisionTriangle>,
    ) -> Result<(), super::RuntimeStaticMovementError> {
        output.clear();
        let active = self
            .active
            .as_mut()
            .ok_or(super::RuntimeStaticMovementError::InvalidReference)?;
        for index in 0..active.movement.roots.len() {
            let reference = active.movement.roots[index];
            active
                .registration_root_mut(reference)
                .map_err(|_| super::RuntimeStaticMovementError::InvalidReference)?
                .append_liquid_movement(bounds, output)?;
        }
        if active.terrain.global_world_model().is_none() {
            for (tile, chunk) in bounds.terrain_chunks()? {
                if let Some(tile) = active.tile_at(tile) {
                    solarity_systems::append_terrain_liquid_movement(
                        &tile.decoded,
                        chunk,
                        bounds,
                        output,
                    )?;
                }
            }
        }
        // 75FF90 negates every plane, retaining each authored vertex order.
        for triangle in output {
            *triangle = solarity_systems::MovementCollisionTriangle::with_normal(
                *triangle.vertices(),
                -triangle.normal(),
            )?;
        }
        Ok(())
    }

    /// Runs 7A1BC0's registered-unit liquid selection. An interior uses its first
    /// registered group; exterior registration uses the general WMO/ADT query.
    ///
    /// # Errors
    /// Returns malformed resident geometry or missing liquid behavior data.
    pub fn unit_submerged_liquid(
        &mut self,
        position: Vec3,
        liquids: &LiquidTypeCatalog,
    ) -> Result<Option<SubmergedLiquid>, RuntimeMovementRegistrationError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        let selection = active.unit_registration(position)?;
        if selection
            .selected()
            .is_some_and(|candidate| candidate.hit().is_interior())
        {
            // 7C2A70 publishes primary bank references in order. 7A13E0 picks
            // the first WMO reference, including a transformed root.
            if let Some(candidate) = selection.primary().into_iter().flatten().next() {
                let reference = active
                    .movement
                    .roots
                    .iter()
                    .find(|root| root.owner() == candidate.owner())
                    .copied()
                    .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
                return Ok(active
                    .registration_root_mut(reference)?
                    .registered_unit_liquid(candidate.hit().group_index(), position, liquids)?);
            }
        }
        active.general_submerged_liquid(position, liquids)
    }

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
        active.general_submerged_liquid(position, liquids)
    }
}

impl super::ResidentTerrainMap {
    /// General 7A0B00 tries each eligible WMO before falling back to ADT.
    fn general_submerged_liquid(
        &mut self,
        position: Vec3,
        liquids: &LiquidTypeCatalog,
    ) -> Result<Option<SubmergedLiquid>, RuntimeMovementRegistrationError> {
        let active = self;
        for index in 0..active.movement.roots.len() {
            let root = active.movement.roots[index];
            if let Some(liquid) = active
                .registration_root_mut(root)?
                .submerged_liquid(position, liquids)?
            {
                return Ok(Some(liquid));
            }
        }
        let point = TerrainRegistrationPoint::new(position.x, position.y)?;
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
