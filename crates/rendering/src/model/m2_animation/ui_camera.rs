//! Model widget camera selection and the camera-less stock UI projection.

use glam::{Mat4, Vec3};
use solarity_asset::M2AnimationSet;

use crate::{WorldCamera, WorldCameraFrame};

use super::{M2AnimationClock, M2CameraEffectScale, M2CameraFrameError, sample_m2_camera_frame};

/// Layout inputs needed by a Model widget's default orthographic projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2UiCameraViewport {
    size: [f32; 2],
    screen_size: [f32; 2],
    effective_scale: f32,
}

impl M2UiCameraViewport {
    /// Supplies viewport and root-screen extents in the same logical units,
    /// plus the widget's effective scale after parent composition.
    #[must_use]
    pub const fn new(size: [f32; 2], screen_size: [f32; 2], effective_scale: f32) -> Self {
        Self {
            size,
            screen_size,
            effective_scale,
        }
    }

    fn validate(self) -> Result<(), M2CameraFrameError> {
        if self
            .size
            .into_iter()
            .chain(self.screen_size)
            .chain([self.effective_scale])
            .any(|value| !value.is_finite() || value <= 0.0)
        {
            return Err(M2CameraFrameError::UiViewport);
        }
        Ok(())
    }

    fn default_camera(self) -> Result<WorldCameraFrame, M2CameraFrameError> {
        // 0x0047BF90 normalizes the screen diagonal; 0x0095FBA0 multiplies
        // model scale by normalized screen height, 5/3, and effective UI scale.
        // Keep that common unit conversion in the projection, so the existing
        // mutable model transform and its effects remain in authored M2 units.
        const MODEL_SCALE_FACTOR: f32 = 5.0 / 3.0;
        const NATIVE_DEPTH: f32 = 500.0;
        let screen_aspect = self.screen_size[0] / self.screen_size[1];
        let native_screen_height = 1.0 / screen_aspect.hypot(1.0);
        let native_model_scale = MODEL_SCALE_FACTOR * native_screen_height * self.effective_scale;
        let ui_to_model = 1.0 / (MODEL_SCALE_FACTOR * self.screen_size[1] * self.effective_scale);
        let width = self.size[0] * ui_to_model;
        let height = self.size[1] * ui_to_model;
        let depth = NATIVE_DEPTH / native_model_scale;
        // 0x0095FC30 supplies the viewport's bottom-left origin to 0x004BEE60.
        // Its orthographic matrix and centered view translation are equivalent
        // to these asymmetric bounds. Native scene eye position remains zero.
        WorldCamera::orthographic(
            Vec3::ZERO,
            Vec3::NEG_Z,
            Vec3::Y,
            [0.0, width],
            [0.0, height],
            -depth,
            depth,
        )
        .frame(self.size[0] / self.size[1])
        .map_err(M2CameraFrameError::from)
    }
}

/// Selects an authored camera or the stock camera-less Model widget projection.
///
/// Native `0x0095F9F0` clears the selected camera when its unsigned index is
/// outside the model's table. This includes negative Lua indices and models
/// without any authored cameras. Malformed selected camera tracks remain errors.
/// The returned effect scale belongs to the selected projection path.
///
/// # Errors
///
/// Returns an error for invalid viewport geometry or selected camera data.
pub fn sample_m2_ui_camera_frame(
    animations: &M2AnimationSet,
    camera_index: i32,
    clock: M2AnimationClock,
    viewport: M2UiCameraViewport,
    model_transform: Mat4,
) -> Result<(WorldCameraFrame, M2CameraEffectScale), M2CameraFrameError> {
    viewport.validate()?;
    if let Ok(index) = usize::try_from(camera_index)
        && index < animations.cameras().len()
    {
        let frame = sample_m2_camera_frame(
            animations,
            index,
            clock,
            viewport.size[0] / viewport.size[1],
            model_transform,
        )?;
        let effect_scale = M2CameraEffectScale::from_native_camera(&frame);
        Ok((frame, effect_scale))
    } else {
        Ok((
            viewport.default_camera()?,
            M2CameraEffectScale::EXTERNAL_CAMERA,
        ))
    }
}
