//! Shared world camera composition for terrain demand and presentation.

use solarity_rendering::{WorldCamera, WorldCameraFrame};
use solarity_systems::TerrainStreamingWindow;

use super::ClientServices;
use crate::application::{ApplicationError, RuntimeTerrainError};

/// One frame's completed camera and the state after native feedback publication.
pub(super) struct ResolvedCameraFrame {
    inputs: CameraInputs,
    camera: WorldCameraFrame,
}

/// Clock reuse is restricted to this service frame; changes to any camera or
/// geometry provider force a new solve, including newly admitted terrain.
#[derive(PartialEq)]
struct CameraInputs {
    pose: solarity_systems::PlayerCameraPose,
    settings: solarity_systems::PlayerCameraObstructionSettings,
    terrain_revision: u64,
    extent: (u32, u32),
    far_clip: f32,
    pivot_pitch: f32,
}

impl ClientServices {
    /// Resolves the final camera from admitted environment, player, and collision.
    pub(super) fn resolved_world_camera(
        &mut self,
    ) -> Result<Option<WorldCameraFrame>, ApplicationError> {
        if let Some(cached) = &self.world_camera_frame
            && self.camera_inputs().as_ref() == Some(&cached.inputs)
        {
            return Ok(Some(cached.camera));
        }
        self.world_camera_frame = None;
        let now = crate::platform::client_milliseconds();
        self.player.sample_camera_collision(now)?;
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
            self.world_camera_frame = Some(ResolvedCameraFrame { inputs, camera });
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
    pub(super) fn service_terrain_streaming(&mut self) -> Result<(), ApplicationError> {
        self.world_camera_frame = None;
        let Some(environment) = self.environment.current() else {
            return Ok(());
        };
        let Some(camera) = self.resolved_world_camera()? else {
            return Ok(());
        };
        if self
            .terrain
            .active_map()
            .is_none_or(|map| map.global_world_model().is_some())
        {
            return Ok(());
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
        self.terrain.synchronize_streaming_with_admission(
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
                frame
                    .admit_tile(&mut self.renderer, tile)
                    .map_err(RuntimeTerrainError::from)
            },
        )?;
        if let Some(frame) = self
            .terrain_frame
            .as_mut()
            .filter(|frame| frame.belongs_to_map(environment.map_id()))
            && let Some(primary) = self.terrain.resident_tile().map(|tile| tile.index())
        {
            frame.synchronize_tiles(
                &mut self.renderer,
                primary,
                self.terrain.resident_tiles(),
                &mut self.crt_rand,
            )?;
        }
        Ok(())
    }
}
