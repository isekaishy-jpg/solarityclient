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
        let (Some(environment), Some(pose)) =
            (self.environment.current(), self.player.camera_pose())
        else {
            return Ok(None);
        };
        let (width, height) = self.platform.pixel_extent();
        let aspect_ratio = width as f32 / height as f32;
        // Registered camera collision defaults, pending the live settings owner.
        let pose = self
            .terrain
            .resolve_player_camera(pose, aspect_ratio, true, true)?;
        Ok(Some(
            WorldCamera::stock_following(
                pose.eye(),
                pose.target(),
                pose.up(),
                pose.orbit_pivot(),
                pose.subject(),
                environment.view_distance().value(),
            )
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
