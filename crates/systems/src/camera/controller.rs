//! Player and world camera control policy.

use glam::Vec3;
use solarity_asset::DecodedM2Model;
use solarity_ecs::{PlayerViewState, WorldTransform};

use super::{
    CameraSubjectGeometry, CameraSubjectHeight, CameraSubjectHeightError,
    CameraSubjectHeightSource, PlayerCameraHeightSample, PlayerCameraPose, PlayerCameraPoseError,
};

/// Live orbit pitch bounds recovered from build 12340's `CGCamera` paths.
const CAMERA_PITCH_MINIMUM_RADIANS: f32 = -1.553_343;
const CAMERA_PITCH_MAXIMUM_RADIANS: f32 = 1.553_343;

/// Final stock cap after the distance CVar and its factor are combined.
const CAMERA_DISTANCE_MAXIMUM: f32 = 50.0;

/// Logical first person retains a direction inside the stock near plane.
const CAMERA_ORBIT_DISTANCE_MINIMUM: f32 = 0.01;

/// The orbit builder does not admit a pivot directly on or below the unit.
const CAMERA_PIVOT_HEIGHT_MINIMUM: f32 = 0.1;

/// Attachment identifier read by `CGUnit_C::GetCameraHeight` in build 12340.
const BREATH_ATTACHMENT_ID: u32 = 17;

/// Native offset added above the authored Breath attachment.
const BREATH_CAMERA_OFFSET: f32 = 0.097_222_224;

/// Stock clamps the final unit camera height to this closed interval.
const CAMERA_HEIGHT_MINIMUM: f32 = 0.833_333_3;
const CAMERA_HEIGHT_MAXIMUM: f32 = 15.0;

/// Minimum scale retained by the model presentation transform.
const MODEL_SCALE_MINIMUM: f32 = 0.001;

/// Resolves the build-12340 ordinary player orbit before camera collision.
///
/// The camera target is one unit along the final view direction, not the
/// character or orbit pivot. Pitch and distance use the executable's live
/// clamps; invalid non-finite state is rejected before trigonometry can
/// contaminate renderer and world-residency inputs.
///
/// # Errors
///
/// Returns [`PlayerCameraPoseError`] when the authoritative transform, saved
/// view, or authored subject height contains a non-finite value.
pub fn resolve_player_camera_pose(
    transform: WorldTransform,
    view: PlayerViewState,
    subject_height: CameraSubjectHeight,
) -> Result<PlayerCameraPose, PlayerCameraPoseError> {
    resolve_player_camera_pose_with_flying_mount_height(transform, view, subject_height, 0.0)
}

/// Resolves the mounted player orbit while retaining `$CFM` for obstruction.
///
/// The flying-mount height is deliberately not added to the orbit pivot. The
/// stock client applies it along the camera up vector, then attenuates that
/// displacement with the obstruction distance.
///
/// # Errors
///
/// Returns [`PlayerCameraPoseError`] when either sampled height or the
/// authoritative transform/view state is non-finite.
pub fn resolve_mounted_player_camera_pose(
    transform: WorldTransform,
    view: PlayerViewState,
    heights: PlayerCameraHeightSample,
) -> Result<PlayerCameraPose, PlayerCameraPoseError> {
    resolve_player_camera_pose_with_flying_mount_height(
        transform,
        view,
        heights.subject_height(),
        heights.flying_mount_height(),
    )
}

fn resolve_player_camera_pose_with_flying_mount_height(
    transform: WorldTransform,
    view: PlayerViewState,
    subject_height: CameraSubjectHeight,
    flying_mount_height: f32,
) -> Result<PlayerCameraPose, PlayerCameraPoseError> {
    let subject = transform.position();
    if !subject.is_finite() || !transform.orientation().is_finite() {
        return Err(PlayerCameraPoseError::NonFiniteTransform);
    }
    if !view.distance().is_finite()
        || !view.pitch_radians().is_finite()
        || !view.yaw_offset_radians().is_finite()
    {
        return Err(PlayerCameraPoseError::NonFiniteView);
    }
    if !subject_height.value().is_finite() {
        return Err(PlayerCameraPoseError::NonFiniteSubjectHeight);
    }
    if !flying_mount_height.is_finite() {
        return Err(PlayerCameraPoseError::NonFiniteFlyingMountHeight);
    }

    let yaw = transform.orientation() + view.yaw_offset_radians();
    if !yaw.is_finite() {
        return Err(PlayerCameraPoseError::NonFiniteView);
    }
    let pitch = view
        .pitch_radians()
        .clamp(CAMERA_PITCH_MINIMUM_RADIANS, CAMERA_PITCH_MAXIMUM_RADIANS);
    let distance = view.distance().clamp(0.0, CAMERA_DISTANCE_MAXIMUM);
    let orbit_distance = distance.max(CAMERA_ORBIT_DISTANCE_MINIMUM);
    let facing = Vec3::new(yaw.cos(), yaw.sin(), 0.0);
    let horizontal_distance = orbit_distance * pitch.cos();
    let orbit_pivot = subject
        + Vec3::new(
            0.0,
            0.0,
            subject_height.value().max(CAMERA_PIVOT_HEIGHT_MINIMUM),
        );
    let eye = orbit_pivot - facing * horizontal_distance + Vec3::Z * (orbit_distance * pitch.sin());
    let up = facing * pitch.sin() + Vec3::Z * pitch.cos();
    // `CGCamera` leaves logical first person untouched. In third person the
    // subsequent obstruction interpolation scales this complete displacement,
    // reproducing the executable's distance/max-distance factor.
    let flying_mount_offset = if distance > 0.0 {
        up * flying_mount_height
    } else {
        Vec3::ZERO
    };
    let eye = eye + flying_mount_offset;
    let target = eye + facing * pitch.cos() - Vec3::Z * pitch.sin();

    Ok(PlayerCameraPose::new(
        eye,
        target,
        up,
        orbit_pivot,
        subject,
        flying_mount_height,
    ))
}

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
