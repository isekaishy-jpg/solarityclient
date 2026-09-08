//! Camera-only 77F330 scene collection over retained native owner generations.

use solarity_systems::{PlayerCameraVolume, PlayerCameraVolumeKind};

use super::{RuntimeStaticMovementQuery, RuntimeTerrainCoordinator};
use crate::application::terrain_coordinator::RuntimeCameraSceneError;

#[derive(Default)]
pub(in crate::application::terrain_coordinator) struct CameraGeometry {
    query: RuntimeStaticMovementQuery,
    cells: Vec<[u16; 2]>,
}

impl RuntimeTerrainCoordinator {
    pub(in crate::application::terrain_coordinator) fn camera_world_model_fraction(
        &mut self,
        start: glam::Vec3,
        end: glam::Vec3,
        maximum_fraction: f32,
    ) -> Result<Option<f32>, RuntimeCameraSceneError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        let mut nearest = maximum_fraction;
        let mut found = false;
        for index in 0..active.movement.roots.len() {
            let reference = active.movement.roots[index];
            if let Some(fraction) = active
                .registration_root_mut(reference)?
                .trace_solid_camera(start, end, nearest)?
            {
                nearest = fraction;
                found = true;
            }
        }
        Ok(found.then_some(nearest))
    }

    pub(in crate::application::terrain_coordinator) fn camera_water_fraction(
        &mut self,
        start: glam::Vec3,
        end: glam::Vec3,
        maximum: f32,
    ) -> Result<Option<f32>, RuntimeCameraSceneError> {
        let ray = solarity_systems::PlayerCameraWaterSegment::new(start, end)?;
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        let cells = &mut self.camera_geometry.cells;
        let mut nearest = maximum;
        let mut found = false;
        // 77F310 visits WMO roots before terrain; equal fractions keep the
        // previously selected owner. No generated movement bank is needed.
        for index in 0..active.movement.roots.len() {
            let reference = active.movement.roots[index];
            let root = active.registration_root_mut(reference)?;
            if let Some(fraction) = root.trace_liquid_camera(start, end, nearest, cells)? {
                nearest = fraction;
                found = true;
            }
        }
        if active.terrain.global_world_model().is_none() {
            ray.terrain_cells(cells);
            for &[x, y] in cells.iter() {
                if x > 8192 || y > 8192 {
                    break;
                }
                let tile_index = solarity_asset::TerrainTileIndex::new(
                    ((y >> 7) & 63) as u8,
                    ((x >> 7) & 63) as u8,
                )
                .ok_or(solarity_systems::MovementCollectionError::InvalidBounds)?;
                let chunk = solarity_asset::TerrainChunkIndex::new(
                    ((y >> 3) & 15) as u8,
                    ((x >> 3) & 15) as u8,
                )
                .ok_or(solarity_systems::MovementCollectionError::InvalidBounds)?;
                let Some(tile) = active.tile_at(tile_index) else {
                    break;
                };
                if let Some(fraction) =
                    ray.terrain_cell_fraction(&tile.decoded, chunk, [x & 7, y & 7], nearest)?
                {
                    nearest = fraction;
                    found = true;
                }
            }
        }
        Ok(found.then_some(nearest))
    }

    pub(in crate::application::terrain_coordinator) fn camera_volume_retreat(
        &mut self,
        volume: &PlayerCameraVolume,
        kind: PlayerCameraVolumeKind,
    ) -> Result<Option<f32>, RuntimeCameraSceneError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        let output = &mut self.camera_geometry.query;
        output.clear();
        let bounds = volume.bounds();
        // 7A5330 visits MCNKs first. Missing/loading owners contribute no faces
        // to a camera query; movement's residency-failure contract is separate.
        if active.terrain.global_world_model().is_none() {
            for (index, chunk) in bounds.terrain_chunks()? {
                let Some(tile) = active.tile_at(index) else {
                    continue;
                };
                match kind {
                    PlayerCameraVolumeKind::Water => {
                        solarity_systems::append_terrain_liquid_movement(
                            &tile.decoded,
                            chunk,
                            bounds,
                            &mut output.triangles,
                        )?;
                    }
                    PlayerCameraVolumeKind::Solid => {
                        tile.collision.append_movement_chunk(
                            chunk,
                            bounds,
                            &mut output.triangles,
                        )?;
                        let references = &tile.movement_references.chunks
                            [usize::from(chunk.y()) * 16 + usize::from(chunk.x())];
                        for &index in references {
                            output.append_m2(&tile.m2_scene, &tile.m2_collision, index, bounds)?;
                        }
                    }
                }
            }
        }
        // 7A4EE0 then visits placed WMO roots. This volume path does not append
        // MODD or generic GameObject M2 callbacks from the movement collector.
        for index in 0..active.movement.roots.len() {
            let reference = active.movement.roots[index];
            let root = active.registration_root_mut(reference)?;
            match kind {
                PlayerCameraVolumeKind::Water => {
                    root.append_liquid_camera_volume(volume, &mut output.triangles)?
                }
                PlayerCameraVolumeKind::Solid => {
                    root.append_camera_volume(volume, &mut output.triangles)?
                }
            }
        }
        let mut maximum: Option<f32> = None;
        for triangle in &output.triangles {
            if let Some(retreat) = volume.triangle_retreat(*triangle.vertices())? {
                maximum = Some(maximum.map_or(retreat, |current| current.max(retreat)));
            }
        }
        Ok(maximum)
    }
}
