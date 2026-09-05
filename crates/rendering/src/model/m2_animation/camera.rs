//! Model-authored camera sampling for Glue and portrait viewports.

use glam::{Mat4, Quat, Vec3};
use solarity_asset::M2AnimationSet;
use thiserror::Error;

use crate::{WorldCamera, WorldCameraError, WorldCameraFrame};

use super::sample::sample_spline;
use super::{M2AnimationClock, M2BonePoseError};

/// View-model scale inherited by native-camera M2 particles and ribbons.
///
/// Build 12340 authors M2 camera FOV against a 4:3 diagonal and carries the
/// viewport correction in its native-camera view-model matrix. Dynamic M2
/// effects therefore inherit this scale even though ordinary model geometry
/// does not. External world and character cameras have an identity factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2CameraEffectScale(f32);

impl M2CameraEffectScale {
    /// Identity scale for M2s viewed through an external world camera.
    pub const EXTERNAL_CAMERA: Self = Self(1.0);

    /// Recovers the native-camera view-model scale for a validated frame.
    #[must_use]
    pub fn from_native_camera(frame: &WorldCameraFrame) -> Self {
        const AUTHORED_ASPECT_RATIO: f32 = 4.0 / 3.0;

        Self(1.0_f32.hypot(AUTHORED_ASPECT_RATIO) / 1.0_f32.hypot(frame.aspect_ratio()))
    }

    /// Returns the finite positive multiplier applied to effect axes.
    #[must_use]
    pub const fn factor(self) -> f32 {
        self.0
    }
}

/// A decoded M2 camera cannot form its stock presentation frame.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum M2CameraFrameError {
    /// Widget/root viewport extents or effective scale are invalid.
    #[error("M2 UI camera viewport extents and scale must be positive and finite")]
    UiViewport,
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
/// Eye and target inherit the owning model's affine world transform, as in stock
/// `0x828a00`; roll and clipping distances remain camera properties.
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
    model_transform: Mat4,
) -> Result<WorldCameraFrame, M2CameraFrameError> {
    let clock = clock.resolve(animations)?;
    let camera = animations
        .cameras()
        .get(camera_index)
        .ok_or(M2CameraFrameError::CameraIndex {
            requested: camera_index,
            available: animations.cameras().len(),
        })?;
    let position =
        camera.position_base() + sample_spline(animations, camera.position(), clock, Vec3::ZERO);
    let target = camera.target_position_base()
        + sample_spline(animations, camera.target_position(), clock, Vec3::ZERO);
    let position = model_transform.transform_point3(position);
    let target = model_transform.transform_point3(target);
    let forward = (target - position).normalize_or_zero();
    let mut up = Vec3::Z - forward * Vec3::Z.dot(forward);
    if !up.is_finite() || up.length_squared() <= 1.0e-8 {
        up = Vec3::Y - forward * Vec3::Y.dot(forward);
    }
    up = up.normalize_or_zero();
    let roll = sample_spline(animations, camera.roll_radians(), clock, 0.0);
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
