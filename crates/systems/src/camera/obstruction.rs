//! Stock third-person camera volume traces and obstruction retreat.

use glam::Vec3;
use thiserror::Error;

use super::PlayerCameraPose;

const WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS: f32 = 0.942_477_8;
const WORLD_NEAR_CLIP: f32 = 0.2;
const CENTER_COLLISION_RETREAT: f32 = 0.111_111_11;
const VOLUME_CONTACT_RETREAT: f32 = 0.001;
const CAMERA_VOLUME_EXPANSIONS: [f32; 2] = [1.0, 1.75];

/// A malformed camera or provider result during obstruction resolution.
#[derive(Debug, Error)]
pub enum PlayerCameraObstructionError<E> {
    /// Viewport width divided by height is not positive and finite.
    #[error("camera obstruction aspect ratio is invalid")]
    InvalidAspectRatio,
    /// Pre-collision camera vectors cannot form the required basis.
    #[error("camera obstruction basis is invalid")]
    InvalidCameraBasis,
    /// A provider returned a fraction outside its requested segment interval.
    #[error("camera obstruction trace returned an invalid fraction")]
    InvalidTraceFraction,
    /// An owning scene provider rejected a trace.
    #[error("camera obstruction trace failed")]
    Trace(#[source] E),
}

/// Resolves the nearest stock camera obstruction from an owning scene trace.
///
/// The center ray retreats by one ninth of a world unit. Four near-plane
/// corner rays at ordinary and 1.75 expansion then keep the complete camera
/// volume on the visible side of terrain, WMO, and admitted M2 geometry.
/// `smart_pivot` retains the requested view direction after moving the eye;
/// the disabled branch points the collided eye back toward its orbit pivot.
///
/// # Errors
///
/// Returns [`PlayerCameraObstructionError`] when the camera basis, aspect,
/// provider result, or provider operation is invalid.
pub fn resolve_player_camera_obstruction<E>(
    pose: PlayerCameraPose,
    aspect_ratio: f32,
    smart_pivot: bool,
    mut trace: impl FnMut(Vec3, Vec3, f32) -> Result<Option<f32>, E>,
) -> Result<PlayerCameraPose, PlayerCameraObstructionError<E>> {
    if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
        return Err(PlayerCameraObstructionError::InvalidAspectRatio);
    }
    let pivot = pose.orbit_pivot();
    let desired = pose.eye();
    let view_delta = pose.target() - desired;
    let ray = desired - pivot;
    let view_length_squared = view_delta.length_squared();
    let ray_length = ray.length();
    if !pivot.is_finite()
        || !desired.is_finite()
        || !pose.up().is_finite()
        || !view_length_squared.is_finite()
        || view_length_squared <= 1.0e-10
        || !ray_length.is_finite()
    {
        return Err(PlayerCameraObstructionError::InvalidCameraBasis);
    }
    if ray_length < 0.001 {
        return Ok(pose);
    }
    let view_direction = view_delta / view_length_squared.sqrt();
    let up = orthogonal_up(view_direction, pose.up())
        .ok_or(PlayerCameraObstructionError::InvalidCameraBasis)?;
    let right = view_direction.cross(up).normalize();

    let mut visible_fraction = 1.0;
    if let Some(hit) =
        trace(pivot, desired, visible_fraction).map_err(PlayerCameraObstructionError::Trace)?
    {
        validate_trace_fraction(hit, visible_fraction)?;
        visible_fraction = (hit - CENTER_COLLISION_RETREAT / ray_length).max(0.0);
    }

    for expansion in CAMERA_VOLUME_EXPANSIONS {
        for offset in near_plane_offsets(view_direction, right, up, aspect_ratio, expansion) {
            if let Some(hit) = trace(pivot + offset, desired + offset, visible_fraction)
                .map_err(PlayerCameraObstructionError::Trace)?
            {
                validate_trace_fraction(hit, visible_fraction)?;
                visible_fraction =
                    visible_fraction.min((hit - VOLUME_CONTACT_RETREAT / ray_length).max(0.0));
            }
        }
    }

    let eye = pivot + ray * visible_fraction;
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
        pose.flying_mount_height(),
    ))
}

/// Rejects scene-provider output rather than clamping corrupted trace state.
fn validate_trace_fraction<E>(
    fraction: f32,
    maximum: f32,
) -> Result<(), PlayerCameraObstructionError<E>> {
    if fraction.is_finite() && fraction >= 0.0 && fraction <= maximum {
        Ok(())
    } else {
        Err(PlayerCameraObstructionError::InvalidTraceFraction)
    }
}

/// Projects the continuous orbit up vector onto the final view plane.
fn orthogonal_up(forward: Vec3, supplied_up: Vec3) -> Option<Vec3> {
    let up = supplied_up - forward * supplied_up.dot(forward);
    let length_squared = up.length_squared();
    if !length_squared.is_finite() || length_squared <= 1.0e-8 {
        return None;
    }
    Some(up / length_squared.sqrt())
}

/// Produces four swept near-plane corner offsets in stable winding order.
fn near_plane_offsets(
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    aspect_ratio: f32,
    expansion: f32,
) -> [Vec3; 4] {
    let half_height =
        WORLD_NEAR_CLIP * (WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS * 0.5).tan() * expansion;
    let half_width = half_height * aspect_ratio;
    let center = forward * WORLD_NEAR_CLIP;
    [
        center - right * half_width - up * half_height,
        center + right * half_width - up * half_height,
        center + right * half_width + up * half_height,
        center - right * half_width + up * half_height,
    ]
}
