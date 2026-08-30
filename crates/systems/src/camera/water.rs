//! Stock camera waterline clearance after scene obstruction.

use thiserror::Error;

use super::PlayerCameraPose;

const CAMERA_WATER_CLEARANCE: f32 = 0.05;

/// A malformed camera or liquid provider result during waterline resolution.
#[derive(Debug, Error)]
pub enum PlayerCameraWaterError<E> {
    /// Camera vectors cannot form the required final view direction.
    #[error("camera water-collision basis is invalid")]
    InvalidCameraBasis,
    /// A provider returned a NaN or infinite liquid height.
    #[error("camera liquid-surface query returned a non-finite height")]
    InvalidSurfaceHeight,
    /// The owning scene provider rejected a liquid query.
    #[error("camera liquid-surface query failed")]
    Surface(#[source] E),
}

/// Keeps a collided player camera on the subject's side of a liquid surface.
///
/// Build 12340 samples both the elevated orbit pivot and the already-collided
/// eye. If both points have an admitted surface, a dry subject keeps the eye
/// at least 0.05 units above it while an underwater subject keeps it at least
/// 0.05 units below. Disabled water collision performs no surface queries.
///
/// # Errors
///
/// Returns [`PlayerCameraWaterError`] when the camera basis, provider output,
/// or provider operation is invalid.
pub fn resolve_player_camera_water_collision<E>(
    pose: PlayerCameraPose,
    enabled: bool,
    smart_pivot: bool,
    mut liquid_surface: impl FnMut(f32, f32, f32) -> Result<Option<f32>, E>,
) -> Result<PlayerCameraPose, PlayerCameraWaterError<E>> {
    if !enabled {
        return Ok(pose);
    }
    let pivot = pose.orbit_pivot();
    let desired = pose.eye();
    let view_delta = pose.target() - desired;
    let view_length_squared = view_delta.length_squared();
    if !pivot.is_finite()
        || !desired.is_finite()
        || !pose.up().is_finite()
        || !view_length_squared.is_finite()
        || view_length_squared <= 1.0e-10
    {
        return Err(PlayerCameraWaterError::InvalidCameraBasis);
    }
    let view_direction = view_delta / view_length_squared.sqrt();
    let subject_surface =
        liquid_surface(pivot.x, pivot.y, pivot.z).map_err(PlayerCameraWaterError::Surface)?;
    let eye_surface =
        liquid_surface(desired.x, desired.y, desired.z).map_err(PlayerCameraWaterError::Surface)?;
    validate_surface(subject_surface)?;
    validate_surface(eye_surface)?;
    let (Some(subject_surface), Some(eye_surface)) = (subject_surface, eye_surface) else {
        return Ok(pose);
    };

    let mut eye = desired;
    let subject_underwater = pivot.z < subject_surface;
    if !subject_underwater && eye.z < eye_surface + CAMERA_WATER_CLEARANCE {
        eye.z = eye_surface + CAMERA_WATER_CLEARANCE;
    } else if subject_underwater && eye.z > eye_surface - CAMERA_WATER_CLEARANCE {
        eye.z = eye_surface - CAMERA_WATER_CLEARANCE;
    }
    let target_direction = if smart_pivot {
        view_direction
    } else {
        (pivot - eye).try_normalize().unwrap_or(view_direction)
    };
    Ok(PlayerCameraPose::new(
        eye,
        eye + target_direction,
        pose.up(),
        pivot,
        pose.subject(),
    ))
}

fn validate_surface<E>(surface: Option<f32>) -> Result<(), PlayerCameraWaterError<E>> {
    if surface.is_some_and(|height| !height.is_finite()) {
        Err(PlayerCameraWaterError::InvalidSurfaceHeight)
    } else {
        Ok(())
    }
}
