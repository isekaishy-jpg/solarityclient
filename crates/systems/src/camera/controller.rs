//! Player and world camera control policy.

use solarity_asset::DecodedM2Model;

use super::{
    CameraSubjectGeometry, CameraSubjectHeight, CameraSubjectHeightError, CameraSubjectHeightSource,
};

/// Attachment identifier read by `CGUnit_C::GetCameraHeight` in build 12340.
const BREATH_ATTACHMENT_ID: u32 = 17;

/// Native offset added above the authored Breath attachment.
const BREATH_CAMERA_OFFSET: f32 = 0.097_222_224;

/// Stock clamps the final unit camera height to this closed interval.
const CAMERA_HEIGHT_MINIMUM: f32 = 0.833_333_3;
const CAMERA_HEIGHT_MAXIMUM: f32 = 15.0;

/// Minimum scale retained by the model presentation transform.
const MODEL_SCALE_MINIMUM: f32 = 0.001;

/// Resolves the stock camera pivot height from already-extracted M2 geometry.
///
/// Attachment position is deliberately authored rather than animated. The
/// Breath branch therefore stays stable while the character walks. When the
/// lookup has no ID-17 entry, stock's model branch uses 99% of the authored
/// bounding sphere. Invalid present data is rejected rather than silently
/// switching branches.
///
/// # Errors
///
/// Returns [`CameraSubjectHeightError`] for non-finite scale or attachment
/// data, or for a negative/non-finite model sphere required by the second branch.
pub fn resolve_camera_subject_height(
    geometry: CameraSubjectGeometry,
) -> Result<CameraSubjectHeight, CameraSubjectHeightError> {
    if !geometry.object_scale().is_finite() {
        return Err(CameraSubjectHeightError::NonFiniteScale);
    }
    let scale = geometry.object_scale().max(MODEL_SCALE_MINIMUM);
    let (height, source) = if let Some(attachment_z) = geometry.breath_attachment_z() {
        if !attachment_z.is_finite() {
            return Err(CameraSubjectHeightError::NonFiniteBreathAttachment);
        }
        (
            (attachment_z + BREATH_CAMERA_OFFSET) * scale,
            CameraSubjectHeightSource::BreathAttachment,
        )
    } else {
        if !geometry.model_sphere_radius().is_finite() || geometry.model_sphere_radius() < 0.0 {
            return Err(CameraSubjectHeightError::InvalidModelSphere);
        }
        (
            geometry.model_sphere_radius() * 0.99 * scale,
            CameraSubjectHeightSource::ModelSphere,
        )
    };
    Ok(CameraSubjectHeight::new(
        height.clamp(CAMERA_HEIGHT_MINIMUM, CAMERA_HEIGHT_MAXIMUM),
        source,
    ))
}

/// Extracts the exact authored inputs from a decoded build-12340 M2.
///
/// The model's attachment lookup is authoritative; no linear search is added
/// when slot 17 is absent.
///
/// # Errors
///
/// Returns [`CameraSubjectHeightError`] when the selected authored inputs are
/// invalid for camera arithmetic.
pub fn resolve_model_camera_subject_height(
    model: &DecodedM2Model,
    object_scale: f32,
) -> Result<CameraSubjectHeight, CameraSubjectHeightError> {
    let attachment_z = model
        .attachment(BREATH_ATTACHMENT_ID)
        .map(|attachment| attachment.position().z);
    resolve_camera_subject_height(CameraSubjectGeometry::new(
        attachment_z,
        model.bounds().sphere_radius(),
        object_scale,
    ))
}
