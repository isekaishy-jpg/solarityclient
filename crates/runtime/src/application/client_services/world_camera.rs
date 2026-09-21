//! Shared world camera composition for terrain demand and presentation.

use solarity_rendering::{WorldCamera, WorldCameraFrame};
use solarity_systems::TerrainStreamingWindow;

use super::ClientServices;
use crate::application::{ApplicationError, RuntimeTerrainError, RuntimeTerrainStreamPoll};

/// One frame's completed camera and the state after native feedback publication.
pub(super) struct ResolvedCameraFrame {
    sampled_at_ms: u32,
    inputs: CameraInputs,
    camera: WorldCameraFrame,
}

/// Spatial providers after clock-driven recovery has been sampled. An unchanged
/// sample can reuse collision work even when the client clock has advanced.
#[derive(PartialEq)]
struct CameraInputs {
    pose: solarity_systems::PlayerCameraPose,
    settings: solarity_systems::PlayerCameraObstructionSettings,
    terrain_revision: u64,
    extent: (u32, u32),
    far_clip: f32,
    pivot_pitch: f32,
}

impl ResolvedCameraFrame {
    /// Reuses the current tick without sampling our own collision feedback twice.
    fn camera_for(&self, inputs: Option<&CameraInputs>, now_ms: u32) -> Option<WorldCameraFrame> {
        (self.sampled_at_ms == now_ms && inputs == Some(&self.inputs)).then_some(self.camera)
    }

    /// Clock recovery has already advanced: retain the expensive spatial solve
    /// only if that current sample and every other provider still match.
    fn after_clock_sample(
        &mut self,
        inputs: Option<&CameraInputs>,
        now_ms: u32,
    ) -> Option<WorldCameraFrame> {
        if inputs != Some(&self.inputs) {
            return None;
        }
        self.sampled_at_ms = now_ms;
        Some(self.camera)
    }
}

impl ClientServices {
    /// Resolves the final camera from admitted environment, player, and collision.
    pub(super) fn resolved_world_camera(
        &mut self,
    ) -> Result<Option<WorldCameraFrame>, ApplicationError> {
        let _profile_scope = solarity_profiling::profile!(
            "runtime.application.client_services.world_camera.resolved_world_camera"
        );
        let now = crate::platform::client_milliseconds();
        if let Some(cached) = &self.world_camera_frame
            && let Some(camera) = cached.camera_for(self.camera_inputs().as_ref(), now)
        {
            return Ok(Some(camera));
        }
        // 603D30's collision-height recovery is time-dependent. Terrain demand
        // cannot substitute its earlier sample for presentation, but a new tick
        // alone does not invalidate an unchanged geometric collision result.
        self.player.sample_camera_collision(now)?;
        let inputs = self.camera_inputs();
        if let Some(cached) = &mut self.world_camera_frame
            && let Some(camera) = cached.after_clock_sample(inputs.as_ref(), now)
        {
            return Ok(Some(camera));
        }
        self.world_camera_frame = None;
        let (Some(environment), Some(pose)) =
            (self.environment.current(), self.player.camera_pose())
        else {
            self.player.update_camera_opacity(None);
            return Ok(None);
        };
        let (width, height) = self.platform.pixel_extent();
        let aspect_ratio = width as f32 / height as f32;
        let mut resolved_height = None;
        let mut resolved_distance = None;
        let settings = solarity_systems::PlayerCameraObstructionSettings {
            height_target: self.player.camera_target_height(),
            ..self.player_movement.camera_collision_settings()
        };
        let pose = self.terrain.resolve_player_camera_with_feedback(
            pose,
            aspect_ratio,
            settings,
            |distance, height, contacts| {
                self.player_movement
                    .camera_obstructed(distance, contacts, now);
                resolved_height = Some(height);
                resolved_distance = Some(distance);
            },
        )?;
        self.player.update_camera_opacity(resolved_distance);
        if let Some(height) = resolved_height {
            self.player.camera_height_obstructed(height, now)?;
        }
        let pose = pose
            .with_view_pitch_offset(self.player_movement.camera_pivot_pitch())
            .map_err(super::super::terrain_coordinator::RuntimeCameraError::from)?;
        let camera = WorldCamera::stock_following(
            pose.eye(),
            pose.target(),
            pose.up(),
            pose.orbit_pivot(),
            pose.subject(),
            environment.view_distance().value(),
        )
        .with_view_direction(pose.forward())
        .frame(aspect_ratio)?;
        if let Some(inputs) = self.camera_inputs() {
            self.world_camera_frame = Some(ResolvedCameraFrame {
                sampled_at_ms: now,
                inputs,
                camera,
            });
        }
        Ok(Some(camera))
    }

    /// Captures provider state after feedback so that feedback itself cannot
    /// accidentally schedule a second identical collision traversal.
    fn camera_inputs(&self) -> Option<CameraInputs> {
        Some(CameraInputs {
            pose: self.player.camera_pose()?,
            settings: solarity_systems::PlayerCameraObstructionSettings {
                height_target: self.player.camera_target_height(),
                ..self.player_movement.camera_collision_settings()
            },
            terrain_revision: self.terrain.model_light_revision(),
            extent: self.platform.pixel_extent(),
            far_clip: self.environment.current()?.view_distance().value(),
            pivot_pitch: self.player_movement.camera_pivot_pitch(),
        })
    }

    /// Supplies the resolved camera window while the loading card can still own presentation.
    pub(super) fn service_terrain_streaming(&mut self) -> Result<bool, ApplicationError> {
        let _profile_scope = solarity_profiling::profile!(
            "runtime.application.client_services.world_camera.service_terrain_streaming"
        );
        self.world_camera_frame = None;
        let Some(environment) = self.environment.current() else {
            return Ok(false);
        };
        let Some(camera) = self.resolved_world_camera()? else {
            return Ok(false);
        };
        let Some(map) = self.terrain.active_map() else {
            return Ok(false);
        };
        if map.global_world_model().is_some() {
            return Ok(self
                .terrain_frame
                .as_ref()
                .is_some_and(|frame| frame.belongs_to_map(environment.map_id())));
        }
        // WorldFrame.cpp 0x004FAADF uses the followed object's position during
        // ordinary follow mode, and the camera eye when no object is followed.
        let origin = camera
            .camera()
            .subject()
            .map_or(camera.camera().position(), |subject| subject.position());
        let window = TerrainStreamingWindow::new(
            origin,
            environment.view_distance(),
            camera.terrain_streaming_corner(),
        )
        .map_err(RuntimeTerrainError::from)?;
        // Entry keeps presentation covered. Finish one completed tile's GPU
        // resources there, rather than pacing each resource across visible frames.
        let loading = self.loading_screen.is_some() || self.world_transfer.is_entering_world();
        let poll = self.terrain.synchronize_streaming_with_admission(
            environment.map_id(),
            origin,
            window,
            &self.cpu,
            |tile| {
                let Some(frame) = self
                    .terrain_frame
                    .as_mut()
                    .filter(|frame| frame.belongs_to_map(environment.map_id()))
                else {
                    return Ok(false);
                };
                if loading {
                    frame
                        .admit_loading_tile(
                            &mut solarity_rendering::GpuPreparation::new(
                                &mut self.renderer,
                                &mut crate::application::frame_pipeline::FrameWait::Native(
                                    &mut self.platform,
                                )
                                .recording(&self.cpu),
                            ),
                            tile,
                        )
                        .map_err(RuntimeTerrainError::from)
                } else {
                    frame
                        .admit_tile(
                            &mut solarity_rendering::GpuPreparation::new(
                                &mut self.renderer,
                                &mut crate::application::frame_pipeline::FrameWait::Native(
                                    &mut self.platform,
                                )
                                .recording(&self.cpu),
                            ),
                            tile,
                        )
                        .map_err(RuntimeTerrainError::from)
                }
            },
        )?;
        if let Some(frame) = self
            .terrain_frame
            .as_mut()
            .filter(|frame| frame.belongs_to_map(environment.map_id()))
            && let Some(primary) = self.terrain.resident_tile().map(|tile| tile.index())
        {
            frame.synchronize_tiles(
                &mut solarity_rendering::GpuPreparation::new(
                    &mut self.renderer,
                    &mut crate::application::frame_pipeline::FrameWait::Native(&mut self.platform)
                        .recording(&self.cpu),
                ),
                primary,
                self.terrain.resident_tiles(),
                &mut self.crt_rand,
            )?;
        }
        Ok(poll == RuntimeTerrainStreamPoll::Current)
    }

    /// Prepare visible detail behind the card; ordinary world presentation owns
    /// this work after entry. Workers must also be serviced while the card draws.
    pub(super) fn prepare_world_entry_detail(&mut self) -> Result<bool, ApplicationError> {
        let _profile_scope = solarity_profiling::profile!(
            "runtime.application.client_services.world_camera.prepare_world_entry_detail"
        );
        if self.loading_screen.is_none() && !self.world_transfer.is_entering_world() {
            return Ok(true);
        }
        let Some(camera) = self.resolved_world_camera()? else {
            return Ok(false);
        };
        let settings = ["groundEffectDensity", "groundEffectDist"].map(|name| {
            self.world_ui
                .as_ref()
                .map_or_else(|| self.glue.cvar_number(name), |ui| ui.cvar_number(name))
        });
        let Some(frame) = self.terrain_frame.as_mut() else {
            return Ok(false);
        };
        frame.set_ground_detail(settings[0], settings[1])?;
        frame.prepare_ground_detail(
            &mut solarity_rendering::GpuPreparation::new(
                &mut self.renderer,
                &mut crate::application::frame_pipeline::FrameWait::Native(&mut self.platform)
                    .recording(&self.cpu),
            ),
            self.terrain.resident_tiles(),
            camera,
        )?;
        frame.service_cpu_retirements(&self.cpu)?;
        Ok(frame.ground_detail_ready())
    }
}

#[cfg(test)]
#[path = "../../../tests/application/world_camera.rs"]
mod tests;
