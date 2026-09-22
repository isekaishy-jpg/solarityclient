//! Ordered animation, topology and scene admission before model packet work.

use super::super::super::{
    CrtRand, GameObjectFrameInput, M2Frame, RuntimeTerrainFrameError, WorldCameraFrame,
    WorldFrustum, unit_effects,
};
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;

impl M2Frame {
    /// Preserves callbacks and RNG order, then launches independent root poses.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_frame_scene(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
        frustum: WorldFrustum,
        camera: WorldCameraFrame,
        animation_time_ms: f32,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        unit_effect_callback: Option<&mut unit_effects::UnitEffectEventCallback<'_>>,
        world_lighting: bool,
        mut terrain: Option<&mut RuntimeTerrainCoordinator>,
        shadow_projection: Option<solarity_rendering::WorldShadowProjection>,
        scenery_shadows: Option<
            crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>,
        >,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut frame_profile = solarity_profiling::profile!("m2.scene_admission");
        self.scene_poses
            .retain_layouts(self.placements.as_slice(), &self.sources);
        let frame_seconds = ((animation_time_ms - self.unit_scene_time_ms) * 0.001).max(0.0);
        self.unit_scene_time_ms = animation_time_ms;
        if let Some(terrain) = terrain.as_mut() {
            terrain.prepare_world_scene(camera)?;
        }
        if let Some(game_objects) = game_objects {
            game_objects.advance_scene(animation_time_ms, random)?;
        }
        self.advance_retired_models(animation_time_ms as u32, game_objects);
        self.visible_draws.clear();
        self.shadow_draws.clear();
        self.shadow_admission.clear();
        self.environment_shadow_draws.clear();
        self.environment_shadow_admission.clear();
        self.transparent_elements.clear();
        self.particle_vertices.clear();
        self.particle_indices.clear();
        self.particle_draws.clear();
        self.ribbon_vertices.clear();
        self.ribbon_draws.clear();
        self.triggered_events.clear();
        self.mount_camera_sample = None;
        self.glue_directional_lights.clear();
        self.glue_point_lights.clear();
        self.scene_lighting.clear();
        self.receiver_frame.clear();
        frame_profile.mark("scene setup");
        self.unit_effects
            .publish_loaded(&self.animations, animation_time_ms, random)?;
        self.unit_effects.begin_frame();
        // Current topology excludes every ordinary model before the first
        // effect. Publication can invalidate that boundary before this pass.
        let first_effect = if self.placement_topology_dirty {
            0
        } else {
            self.placement_visibility.effect_start()
        };
        if self
            .unit_effects
            .retire_drained(&mut self.placements, first_effect)
        {
            if !self.placement_topology_dirty {
                self.placement_visibility
                    .replace_effect_tail(&mut self.placements, &self.sources);
            }
            self.compact_sources();
        }
        self.publish_placement_topology();
        let dynamic = self.placement_visibility.dynamic_scene_indices();
        let slots = dynamic.iter().copied().max().map_or(Ok(0), |index| {
            index
                .checked_add(1)
                .ok_or(solarity_cpu::CpuError::StorageSizeOverflow)
        })?;
        self.scene_poses
            .prepare_storage(cpu.storage(), slots, dynamic.len())?;
        self.pose_batch.prepare_storage(
            cpu.storage(),
            self.placements.len(),
            self.placement_visibility.dynamic_indices().len(),
        )?;
        let receivers = self
            .placements
            .len()
            .checked_add(self.unit_effects.pending_placement_count())
            .ok_or(solarity_cpu::CpuError::StorageSizeOverflow)?;
        self.receiver_frame
            .prepare_storage(cpu.storage(), receivers)?;
        // Scene callbacks run before draw admission and may sample offscreen roots.
        // Admit their shared named-bone scratch against all current source bounds.
        self.bone_samples_scratch.reserve_cpu_storage(
            cpu.storage(),
            self.sources
                .iter()
                .flatten()
                .map(|source| source.model.animations().bones().len())
                .max()
                .unwrap_or(0),
        )?;
        frame_profile.mark("residency and topology");
        self.vehicle_passengers.prepare_timing(
            &mut super::super::poses::ScenePoseExecution {
                cpu: Some(cpu),
                wait,
                poses: &mut self.scene_poses,
            },
            self.placements.as_mut_slice(),
            &self.sources,
            &self.placement_visibility,
            &self.requested_items,
            camera.view(),
            animation_time_ms,
            random,
        )?;
        // Prepare unit selection and yaw before placement. Authored events and
        // completion follow below in scene traversal order, before camera culling.
        for &index in self.placement_visibility.dynamic_indices() {
            let placement = &mut self.placements[index];
            if let Some(animation) = &placement.unit_animation {
                if let Some(registration) = placement.scene_registration
                    && let Some(terrain) = terrain.as_mut()
                    && terrain.unit_scene_admits(registration.position, registration.bounds)?
                {
                    animation.admit_scene_collision();
                }
                animation.prepare_scene(animation_time_ms, random)?;
                placement.transform =
                    placement.local_transform * animation.body_pose().placement_rotation;
            }
        }
        // Ground placement follows unit animation/yaw for every model, including
        // mounts inserted before their riders. It does not advance a second
        // unit callback or substitute the rider's model/timer for the mount.
        for &index in self.placement_visibility.dynamic_indices() {
            let placement = &mut self.placements[index];
            let Some(ground) = &placement.ground_placement else {
                continue;
            };
            if ground.owner.passenger_input().is_some() {
                continue;
            }
            if !ground.owner.uses_ground_placement() {
                continue;
            }
            if placement.unit_animation.is_some() {
                placement.transform = ground.owner.ground_transform(
                    ground.position,
                    ground.scale,
                    animation_time_ms,
                    frame_seconds,
                )?;
            } else if let Some(source) = &self.sources[placement.source_index]
                && let Some(playback) = &placement.playback
            {
                placement.transform = ground.owner.ground_model_transform(
                    ground.position,
                    ground.scale,
                    animation_time_ms,
                    frame_seconds,
                    &source.model,
                    &playback.borrow(),
                )?;
            }
        }
        self.vehicle_passengers.prepare(
            &mut super::super::poses::ScenePoseExecution {
                cpu: Some(cpu),
                wait,
                poses: &mut self.scene_poses,
            },
            self.placements.as_mut_slice(),
            &self.sources,
            &self.placement_visibility,
            &self.requested_items,
            camera.view(),
            animation_time_ms,
            random,
        )?;
        self.placement_visibility
            .set_vehicle_parents(self.vehicle_passengers.parents());
        self.seed_callback_poses(cpu, camera, animation_time_ms)?;
        self.advance_unit_callbacks(
            Some(cpu),
            wait,
            camera,
            animation_time_ms,
            random,
            unit_effect_callback,
        )?;
        self.scene_poses.finish(wait)?;
        self.rider_transforms.clear();
        self.rider_transforms.reserve(self.mounted_guids.len());
        self.item_transforms.clear();
        self.item_transforms.reserve(self.requested_items.len());
        self.visual_transforms.clear();
        self.visual_transforms.reserve(self.requested_visuals.len());
        self.glue_attachment_transforms.clear();
        self.glue_attachment_transforms.reserve(
            self.glue_attachment_ids
                .len()
                .saturating_sub(self.glue_attachment_transforms.capacity()),
        );
        frame_profile.mark("dynamic models");
        self.prepare_unit_poses(
            Some(cpu),
            super::super::poses::PoseAdmission::new(
                camera,
                frustum,
                shadow_projection,
                scenery_shadows,
                self.environment_detail,
            ),
            world_lighting,
            animation_time_ms as u32,
        )?;
        frame_profile.mark("unit pose batch");
        if let Some(terrain) = terrain.as_ref() {
            self.doodad_scene.prepare(
                terrain,
                &self.placement_visibility,
                self.placements.as_mut_slice(),
                &self.sources,
                camera.camera().position(),
                self.environment_detail,
            )?;
        }
        frame_profile.mark("WMO doodad admission");
        Ok(())
    }
}
