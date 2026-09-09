//! Shared world camera composition for terrain demand and presentation.

use solarity_rendering::{WorldCamera, WorldCameraFrame};
use solarity_systems::TerrainStreamingWindow;

use super::ClientServices;
use crate::application::{ApplicationError, RuntimeTerrainError};

impl ClientServices {
    /// Resolves the final camera from admitted environment, player, and collision.
    pub(super) fn resolved_world_camera(
        &mut self,
    ) -> Result<Option<WorldCameraFrame>, ApplicationError> {
        let now = crate::platform::client_milliseconds();
        self.player.sample_camera_collision(now)?;
        let (Some(environment), Some(pose)) =
            (self.environment.current(), self.player.camera_pose())
        else {
            return Ok(None);
        };
        let (width, height) = self.platform.pixel_extent();
        let aspect_ratio = width as f32 / height as f32;
        let mut resolved_height = None;
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
            },
        )?;
        if let Some(height) = resolved_height {
            self.player.camera_height_obstructed(height, now)?;
        }
        let pose = pose
            .with_view_pitch_offset(self.player_movement.camera_pivot_pitch())
            .map_err(super::super::terrain_coordinator::RuntimeCameraError::from)?;
        Ok(Some(
            WorldCamera::stock_following(
                pose.eye(),
                pose.target(),
                pose.up(),
                pose.orbit_pivot(),
                pose.subject(),
                environment.view_distance().value(),
            )
            .with_view_direction(pose.forward())
            .frame(aspect_ratio)?,
        ))
    }

    /// Supplies the resolved camera window while the loading card can still own presentation.
    pub(super) fn service_terrain_streaming(&mut self) -> Result<(), ApplicationError> {
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
        self.terrain.synchronize_streaming_async(
            environment.map_id(),
            origin,
            window,
            &self.cpu,
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
