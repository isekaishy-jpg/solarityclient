//! Model-authored camera sampling for Glue and portrait viewports.

use glam::{Quat, Vec3};
use solarity_asset::M2AnimationSet;
use thiserror::Error;

use crate::{WorldCamera, WorldCameraError, WorldCameraFrame};

use super::sample::{sample_angle_radians, sample_vec3};
use super::{M2AnimationClock, M2BonePoseError};

/// A decoded M2 camera cannot form its stock presentation frame.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum M2CameraFrameError {
    /// The widget selected a camera outside the model's authored table.
    #[error("M2 camera {requested} is unavailable; model has {available} cameras")]
    CameraIndex {
        /// Requested zero-based camera slot.
        requested: usize,
        /// Number of decoded camera records.
        available: usize,
    },
    /// Camera tracks share the model's validated sequence clock.
    #[error(transparent)]
    Animation(#[from] M2BonePoseError),
    /// The sampled basis or projection is not finite and non-degenerate.
    #[error(transparent)]
    Camera(#[from] WorldCameraError),
}

/// Samples one model-authored camera into the common renderer frame.
///
/// Build 12340 stores a diagonal field of view in M2 camera records. The
/// native model-camera path converts it for the active viewport aspect before
/// constructing the ordinary right-handed projection.
///
/// # Errors
///
/// Returns [`M2CameraFrameError`] for an unavailable camera or animation
/// sequence, or when sampled camera data cannot form a valid frame.
pub fn sample_m2_camera_frame(
    animations: &M2AnimationSet,
    camera_index: usize,
    clock: M2AnimationClock,
    aspect_ratio: f32,
) -> Result<WorldCameraFrame, M2CameraFrameError> {
    let sequence = clock.resolve(animations)?;
    let camera = animations
        .cameras()
        .get(camera_index)
        .ok_or(M2CameraFrameError::CameraIndex {
            requested: camera_index,
            available: animations.cameras().len(),
        })?;
    let position = camera.position_base()
        + sample_vec3(
            animations,
            camera.position(),
            sequence,
            clock.animation_time_ms(),
            clock.global_time_ms(),
            Vec3::ZERO,
        );
    let target = camera.target_position_base()
        + sample_vec3(
            animations,
            camera.target_position(),
            sequence,
            clock.animation_time_ms(),
            clock.global_time_ms(),
            Vec3::ZERO,
        );
    let forward = (target - position).normalize_or_zero();
    let mut up = Vec3::Z - forward * Vec3::Z.dot(forward);
    if !up.is_finite() || up.length_squared() <= 1.0e-8 {
        up = Vec3::Y - forward * Vec3::Y.dot(forward);
    }
    up = up.normalize_or_zero();
    let roll = sample_angle_radians(
        animations,
        camera.roll_radians(),
        sequence,
        clock.animation_time_ms(),
        clock.global_time_ms(),
        0.0,
    );
    if roll.is_finite() && roll.abs() > 1.0e-6 && forward.length_squared() > 1.0e-8 {
        up = Quat::from_axis_angle(forward, roll) * up;
    }
    let vertical_field_of_view =
        camera.field_of_view_radians() / (1.0 + aspect_ratio * aspect_ratio).sqrt();
    WorldCamera::new(
        position,
        target,
        up,
        vertical_field_of_view,
        camera.near_clip(),
        camera.far_clip(),
    )
    .frame(aspect_ratio)
    .map_err(M2CameraFrameError::from)
}
