//! WMO shadow groups and MODR membership independent of portal visibility.

use glam::{Mat4, Vec3};
use solarity_rendering::{VulkanError, WorldEnvironmentWmoCaster};
use solarity_systems::{MovementCollisionBounds, WorldModelVisibilityError};

use super::super::{RuntimeTerrainFrameError, shadow::WorldModelShadowDoodads};
use super::{WorldModelFrame, WorldModelGpuPlacementOwner};

impl super::preparation::ShadowJob {
    pub(super) fn run(&mut self) -> Result<(), RuntimeTerrainFrameError> {
        self.draws.clear();
        self.doodads.clear();
        let Some(admission) = self.admission.as_ref() else {
            return Ok(());
        };
        let mut group_counts = [0; 4];
        for placement in self.inputs.iter() {
            let moving = matches!(
                placement.owner,
                WorldModelGpuPlacementOwner::GameObject { .. }
            );
            // Static geometry can be idle while its animated MODR doodads
            // still need primary membership, so retain that group traversal.
            let source = &placement.source;
            let transform = placement.transform;
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
                        self.doodads.push((
                            (placement.owner.scene_owner(), usize::from(doodad)),
                            doodad_maps,
                        ))?;
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
                    let draw = source
                        .shadow_templates
                        .get(draw_index)
                        .copied()
                        .ok_or(VulkanError::WorldModelDrawIndex {
                            requested: draw_index,
                            available: source.shadow_templates.len(),
                        })?
                        .with_model(transform);
                    self.draws.push(WorldEnvironmentWmoCaster {
                        draw,
                        maps: geometry_maps,
                        blend_mode: source.plan.materials()[material].blend_mode(),
                    })?;
                }
            }
        }
        Ok(())
    }
}
impl WorldModelFrame {
    pub(in crate::application::terrain_frame) fn shadow_doodads(&self) -> &WorldModelShadowDoodads {
        &self.shadow_doodads
    }
    #[cfg(test)]
    pub(in crate::application::terrain_frame) fn prepare_shadow_draws(
        &mut self,
        _renderer: &solarity_rendering::VulkanRenderer,
        admission: Option<&super::super::shadow::WorldShadowAdmission>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let cpu = crate::frame_cpu_support::executor()?;
        self.begin_preparation(&cpu, &[], admission, 0., Vec3::ZERO, Vec3::ZERO)?
            .finish_shadow(&mut crate::application::frame_pipeline::FrameWait::Offline)
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
