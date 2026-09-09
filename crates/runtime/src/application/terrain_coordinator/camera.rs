//! Native primary camera constraints and final water-interface correction.

use solarity_systems::{
    PLAYER_CAMERA_WATER_CLEARANCE, PlayerCameraObstructionSettings, PlayerCameraPose,
    PlayerCameraSceneQuery, PlayerCameraVolumeQueryError, PlayerCameraWaterSegment,
    resolve_player_camera_obstruction, resolve_player_camera_volume,
    resolve_player_camera_water_interface,
};

use super::{
    RuntimeCameraError, RuntimeCameraSceneError, RuntimeTerrainCoordinator, camera_profile,
    nearest_fraction,
};

impl RuntimeTerrainCoordinator {
    /// Resolves a player camera using the current subject registration and CVars.
    ///
    /// # Errors
    /// Returns invalid camera inputs or failures from admitted scene geometry.
    pub fn resolve_player_camera(
        &mut self,
        pose: PlayerCameraPose,
        aspect_ratio: f32,
        settings: PlayerCameraObstructionSettings,
    ) -> Result<PlayerCameraPose, RuntimeCameraError> {
        self.resolve_player_camera_with_feedback(pose, aspect_ratio, settings, |_, _, _| {})
    }

    pub(in crate::application) fn resolve_player_camera_with_feedback(
        &mut self,
        pose: PlayerCameraPose,
        aspect_ratio: f32,
        settings: PlayerCameraObstructionSettings,
        feedback: impl FnOnce(f32, f32, solarity_systems::PlayerCameraContacts),
    ) -> Result<PlayerCameraPose, RuntimeCameraError> {
        let mut profile = self
            .camera_profile
            .as_ref()
            .map(|_| [std::time::Duration::ZERO; 5]);
        let obstruction =
            resolve_player_camera_obstruction(pose, aspect_ratio, settings, |query| match query {
                PlayerCameraSceneQuery::Segment { start, end, water } => {
                    let terrain = camera_profile::measure(&mut profile, 0, || {
                        self.trace_collision(start, end, 0.0, 1.0)
                    })?
                    .map(|hit| hit.fraction());
                    let world_model = camera_profile::measure(&mut profile, 1, || {
                        self.trace_world_model_camera(start, end, 1.0)
                    })?;
                    let m2 = camera_profile::measure(&mut profile, 2, || {
                        self.trace_m2_camera(start, end, 1.0)
                    })?;
                    let solid = nearest_fraction(nearest_fraction(terrain, world_model), m2);
                    let liquid = if water {
                        camera_profile::measure(&mut profile, 3, || {
                            self.camera_water_fraction(start, end, solid.unwrap_or(1.0))
                        })?
                    } else {
                        None
                    };
                    Ok::<_, RuntimeCameraSceneError>(nearest_fraction(solid, liquid))
                }
                PlayerCameraSceneQuery::Volume { volume, kind } => {
                    camera_profile::measure(&mut profile, 4, || {
                        self.camera_volume_retreat(volume, kind)
                    })
                }
            })?;
        feedback(
            obstruction.distance(),
            obstruction.height(),
            obstruction.contacts(),
        );
        let pose = obstruction.pose();
        let forward = pose.forward();
        // The final water-only segment is independent of cameraWaterCollision.
        // Resolve it first so the subsequent solid-volume callback has one
        // mutable scene borrow, while Systems retains 6061D0's arithmetic.
        let contact = if obstruction.distance() > 0.0 {
            let start = pose.eye() + glam::Vec3::Z * PLAYER_CAMERA_WATER_CLEARANCE;
            let end = pose.eye() - glam::Vec3::Z * PLAYER_CAMERA_WATER_CLEARANCE;
            let fraction = camera_profile::measure(&mut profile, 3, || {
                self.camera_water_fraction(start, end, 1.0)
            })?;
            let ray =
                PlayerCameraWaterSegment::new(start, end).map_err(RuntimeCameraSceneError::from)?;
            fraction.map(|fraction| ray.contact(fraction))
        } else {
            None
        };
        let eye = resolve_player_camera_water_interface(
            pose.eye(),
            pose.orbit_pivot(),
            forward,
            obstruction.distance(),
            |_, _| Ok(contact),
            |distance, eye, pivot| {
                resolve_player_camera_volume(
                    distance,
                    eye,
                    pivot,
                    aspect_ratio,
                    false,
                    |volume, kind| {
                        camera_profile::measure(&mut profile, 4, || {
                            self.camera_volume_retreat(volume, kind)
                        })
                    },
                )
                .map_err(|error| match error {
                    PlayerCameraVolumeQueryError::Geometry(error) => {
                        RuntimeCameraSceneError::Volume(error)
                    }
                    PlayerCameraVolumeQueryError::Scene(error) => error,
                })
            },
        )?;
        if let (Some(profiler), Some(profile)) = (&mut self.camera_profile, profile) {
            profiler.record(profile);
        }
        Ok(pose.with_eye(eye)?)
    }
}
