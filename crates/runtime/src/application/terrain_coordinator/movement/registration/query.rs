//! Native point and box registration against resident terrain and WMO roots.

use super::super::{
    MovementRootReference, ResidentTerrainMap, RuntimeStaticMovementResidency,
    RuntimeWorldModelMovementOwner, SceneAddress,
};
use super::{
    RuntimeMovementReference, RuntimeMovementRegistrationError, RuntimeMovementRegistrationQuery,
};
use glam::Vec3;
use solarity_systems::{
    MovementBspCacheMode, MovementCollisionBounds, PlacedM2Collision, PlacedWorldModelCollision,
    TerrainRegistrationPoint, WorldModelRegistrationKind, WorldModelRegistrationQuery,
    WorldModelRegistrationSelection,
};

impl ResidentTerrainMap {
    /// Shares the exact Unit_C point query across admission, light, liquid and sound.
    pub(in crate::application::terrain_coordinator::movement) fn unit_registration(
        &mut self,
        position: Vec3,
    ) -> Result<
        WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>,
        RuntimeMovementRegistrationError,
    > {
        if let Some(selection) = self
            .movement
            .unit_registrations
            .get(position, &self.movement.lighting)
        {
            return Ok(selection);
        }
        let selection = self.query_unit_registration(position)?;
        self.movement
            .unit_registrations
            .insert(position, &self.movement.lighting, selection);
        Ok(selection)
    }

    /// Native downward/upward probes and terrain occlusion, unchanged on a miss.
    fn query_unit_registration(
        &mut self,
        position: Vec3,
    ) -> Result<
        WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>,
        RuntimeMovementRegistrationError,
    > {
        let start = position + Vec3::Z * 0.1;
        let end = position - Vec3::Z * 1000.0;
        let address = TerrainRegistrationPoint::new(position.x, position.y)?;
        let terrain_height = self
            .tile_at(address.tile())
            .map(|tile| tile.collision.registration_height_at(address))
            .transpose()?
            .flatten();
        let terrain_fraction = terrain_height
            .map(|height| ((f64::from(start.z) - f64::from(height)) * f64::from(0.001_f32)) as f32)
            .filter(|fraction| *fraction >= 0.0);
        let cache = MovementBspCacheMode::Enabled;
        let mut selection = self.probe_registration_roots(start, end, start, cache)?;
        if terrain_fraction.is_none() && selection.selected().is_none() {
            selection = self.probe_registration_roots(
                position,
                position + Vec3::Z * 1000.0,
                position,
                cache,
            )?;
        }
        if let Some(fraction) = terrain_fraction {
            selection.occlude_by_terrain(fraction)?;
        }
        Ok(selection)
    }

    pub(in crate::application::terrain_coordinator::movement) fn registration_root_mut(
        &mut self,
        root: MovementRootReference,
    ) -> Result<&mut PlacedWorldModelCollision, RuntimeMovementRegistrationError> {
        let reference = match root {
            MovementRootReference::Static(reference) => reference,
            MovementRootReference::GameObject(identity) => {
                return self
                    .movement
                    .game_object_world_models
                    .collision_mut(identity)
                    .ok_or(RuntimeMovementRegistrationError::InvalidReference);
            }
        };
        let models = match reference.scene {
            SceneAddress::Global => {
                &mut self
                    .global_world_model
                    .as_mut()
                    .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                    .world_model_collision
            }
            SceneAddress::Tile(index) => {
                &mut self
                    .tile_at_mut(index)
                    .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                    .world_model_collision
            }
        };
        models
            .instance_mut(reference.placement)
            .ok_or(RuntimeMovementRegistrationError::InvalidReference)
    }

    fn probe_registration_roots(
        &mut self,
        start: Vec3,
        end: Vec3,
        point: Vec3,
        cache: MovementBspCacheMode,
    ) -> Result<
        WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>,
        RuntimeMovementRegistrationError,
    > {
        let mut query = WorldModelRegistrationQuery::new(start, end, point)?;
        for reference in self.root_candidates(start, end)? {
            query.probe_root(
                reference.owner(),
                match reference {
                    MovementRootReference::Static(_) => WorldModelRegistrationKind::Static,
                    MovementRootReference::GameObject(_) => WorldModelRegistrationKind::Transformed,
                },
                self.registration_root_mut(reference)?,
                cache,
            )?;
        }
        Ok(query.finish())
    }

    fn append_registration_root(
        &mut self,
        reference: MovementRootReference,
        render_bounds: MovementCollisionBounds,
        interior: Option<usize>,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        output.groups.clear();
        self.registration_root_mut(reference)?
            .append_registration_groups(render_bounds, interior, &mut output.groups)?;
        output.references.extend(
            output
                .groups
                .iter()
                .map(|&group| reference.destination(group)),
        );
        Ok(())
    }

    /// 7C2040 appends only resident terrain destinations in raw-box order.
    pub(in crate::application::terrain_coordinator::movement) fn append_registration_terrain(
        &self,
        bounds: MovementCollisionBounds,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        if self.terrain.global_world_model().is_none() {
            for (tile_index, chunk) in bounds.registration_terrain_chunks()? {
                if let Some(tile) = self.tile_at(tile_index)
                    && tile.collision.chunk_bounds(chunk).minimum().z <= bounds.maximum().z
                {
                    output.references.push(RuntimeMovementReference::Terrain {
                        tile: tile_index,
                        chunk,
                    });
                }
            }
        }
        Ok(())
    }

    /// 7C2A70 links the primary groups when any WMO registration wins; otherwise
    /// it uses the terrain box. Unlike MapObject registration it scans no extra
    /// outdoor WMO groups around an unregistered unit.
    pub(in crate::application::terrain_coordinator::movement) fn register_unit_liquid(
        &mut self,
        position: Vec3,
        bounds: MovementCollisionBounds,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        let selection = self.unit_registration(position)?;
        if selection.selected().is_some() {
            for candidate in selection.primary().into_iter().flatten() {
                let root = self
                    .movement
                    .roots
                    .iter()
                    .find(|root| root.owner() == candidate.owner())
                    .copied()
                    .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
                output
                    .references
                    .push(root.destination(candidate.hit().group_index()));
            }
        } else {
            self.append_registration_terrain(bounds, output)?;
        }
        output.selection = Some(selection);
        Ok(())
    }

    pub(in crate::application::terrain_coordinator::movement) fn register_game_object_movement(
        &mut self,
        model: &PlacedM2Collision,
        cache: MovementBspCacheMode,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeMovementRegistrationError> {
        let center = model.collision_center();
        let render_bounds = model.render_bounds();
        let start = Vec3::new(
            center.x,
            center.y,
            (f64::from(center.z) + 4.)
                .min(f64::from(render_bounds.maximum().z) + f64::from(0.1_f32)) as f32,
        );
        let end = Vec3::new(center.x, center.y, center.z - 1000.);
        let point = Vec3::new(center.x, center.y, center.z + 0.15);
        let terrain_height = if self.terrain.global_world_model().is_some() {
            if self.global_world_model.is_none() {
                return Ok(RuntimeStaticMovementResidency::PendingMap);
            }
            None
        } else {
            let address = TerrainRegistrationPoint::new(start.x, start.y)?;
            if let Some(tile) = self.tile_at(address.tile()) {
                tile.collision.registration_height_at(address)?
            } else {
                // 7C1660 returns no floor for an absent/loading ADT. This is
                // object registration, not 7A5A60's movement-query admission.
                None
            }
        };
        // 7C28F0 stores this fraction before accepting terrain; height above the
        // segment start does not suppress the fallback upward WMO probe.
        let terrain_fraction = terrain_height
            .map(|height| ((f64::from(start.z) - f64::from(height)) * f64::from(0.001_f32)) as f32)
            .filter(|fraction| *fraction >= 0.);
        let mut selection = self.probe_registration_roots(start, end, point, cache)?;
        if terrain_fraction.is_none() && selection.selected().is_none() {
            selection = self.probe_registration_roots(
                center,
                Vec3::new(center.x, center.y, center.z + 1000.),
                point,
                cache,
            )?;
        }
        // 781A10 maps the GameObject creation flags (0xB) to +7C & 0x2000.
        selection.clear_secondary_bank();
        if let Some(fraction) = terrain_fraction {
            selection.occlude_by_terrain(fraction)?;
        }
        if let Some(candidate) = selection
            .selected()
            .filter(|candidate| candidate.hit().is_interior())
        {
            let reference = self
                .movement
                .roots
                .iter()
                .find(|reference| reference.owner() == candidate.owner())
                .copied()
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            self.append_registration_root(
                reference,
                render_bounds,
                Some(candidate.hit().group_index()),
                output,
            )?;
        } else {
            // 7C2040/7C2139 links only resident chunks and skips missing ADTs.
            // Keep the available destinations even when the box straddles an
            // unloaded tile; scene publication invalidates and rebinds them.
            for index in 0..self.movement.roots.len() {
                let reference = self.movement.roots[index];
                self.append_registration_root(reference, render_bounds, None, output)?;
            }
            self.append_registration_terrain(render_bounds, output)?;
        }
        output.selection = Some(selection);
        Ok(RuntimeStaticMovementResidency::Ready)
    }
}

#[cfg(test)]
#[path = "../../../../../tests/application/unit_registration.rs"]
mod tests;
