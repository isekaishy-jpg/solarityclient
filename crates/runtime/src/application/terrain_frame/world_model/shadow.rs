//! WMO shadow groups and MODR membership independent of portal visibility.

use glam::{Mat4, Vec3};
use solarity_rendering::{VulkanError, VulkanRenderer, WorldEnvironmentWmoCaster};
use solarity_systems::{MovementCollisionBounds, WorldModelVisibilityError};

use super::super::{
    RuntimeTerrainFrameError,
    shadow::{WorldModelShadowDoodads, WorldShadowAdmission},
};
use super::{WorldModelFrame, WorldModelGpuPlacementOwner};

impl WorldModelFrame {
    /// Collects every resident exterior group once per admitted shadow map.
    /// Moving roots use 7B64F0's animated bank; their doodads inherit that bank.
    pub(in crate::application::terrain_frame) fn prepare_shadow_draws(
        &mut self,
        renderer: &VulkanRenderer,
        admission: Option<&WorldShadowAdmission>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.shadow_draws.clear();
        self.shadow_doodads.clear();
        let Some(admission) = admission else {
            return Ok(());
        };
        let mut group_counts = [0; 4];
        for placement in &self.placements {
            if !placement.placement_valid {
                continue;
            }
            let moving = matches!(
                placement.owner,
                WorldModelGpuPlacementOwner::GameObject { .. }
            );
            // Static geometry can be idle while its animated MODR doodads
            // still need primary membership, so retain that group traversal.
            let source = self.sources[placement.source_index].as_ref().ok_or(
                RuntimeTerrainFrameError::WorldModelSourceIndex {
                    source_index: placement.source_index,
                    source_count: self.sources.len(),
                },
            )?;
            let transform = placement.plan.transform();
            let root_maps = admission.admitted_maps(bounds(source.model.bounds(), transform)?);
            if root_maps == 0 {
                continue;
            }
            for (index, group) in source.model.groups().iter().enumerate() {
                if group.flags() & 0x48 == 0 {
                    continue;
                }
                let mut doodad_maps =
                    root_maps & admission.admitted_maps(bounds(group.bounds(), transform)?);
                if moving {
                    doodad_maps = admission.world_model_maps(doodad_maps, moving);
                }
                let mut geometry_maps = admission.world_model_maps(doodad_maps, moving);
                // Native queues cap each map at 2048 WMO groups. The primary
                // queue is collected independently before the environment maps.
                for map in [3, 0, 1, 2] {
                    let bit = 1 << map;
                    if geometry_maps & bit == 0 {
                        continue;
                    }
                    if group_counts[map] == 2048 {
                        geometry_maps &= !bit;
                        if !admission.is_cascaded() {
                            doodad_maps &= !bit;
                        } else if map < 3 {
                            geometry_maps &= 8 | (bit - 1);
                            break;
                        }
                    } else {
                        group_counts[map] += 1;
                    }
                }
                if doodad_maps != 0 {
                    for &doodad in group.doodad_references() {
                        *self
                            .shadow_doodads
                            .entry((placement.owner.scene_owner(), usize::from(doodad)))
                            .or_default() |= doodad_maps;
                    }
                }
                if geometry_maps == 0 {
                    continue;
                }
                let range = source
                    .plan
                    .groups()
                    .get(index)
                    .ok_or(RuntimeTerrainFrameError::WorldModelGroupIndex {
                        group_index: index,
                        group_count: source.plan.groups().len(),
                    })?
                    .shadow_draw_range();
                for draw_index in range {
                    let material =
                        usize::from(source.plan.shadow_draws()[draw_index].material_id());
                    let texture = source
                        .shadow_textures
                        .get(material)
                        .copied()
                        .ok_or(VulkanError::WorldModelDrawMaterial)?;
                    let draw = renderer.prepare_world_model_shadow_draw(
                        source.mesh,
                        texture,
                        &source.plan,
                        draw_index,
                        transform,
                    )?;
                    self.shadow_draws.push(WorldEnvironmentWmoCaster {
                        draw,
                        maps: geometry_maps,
                        blend_mode: source.plan.materials()[material].blend_mode(),
                    });
                }
            }
        }
        Ok(())
    }

    pub(in crate::application::terrain_frame) fn shadow_doodads(&self) -> &WorldModelShadowDoodads {
        &self.shadow_doodads
    }
}

/// Reuses the exact axis-product float stores used by native WMO registration.
fn bounds(
    authored: [[f32; 3]; 2],
    transform: Mat4,
) -> Result<MovementCollisionBounds, RuntimeTerrainFrameError> {
    MovementCollisionBounds::new(Vec3::from_array(authored[0]), Vec3::from_array(authored[1]))
        .and_then(|bounds| bounds.transformed(transform))
        .map_err(WorldModelVisibilityError::from)
        .map_err(Into::into)
}
