//! Registered-subject water state (6049C0) and final eye correction (6061D0).

use glam::Vec3;
use thiserror::Error;

/// Native water-interface probe half-height and clearance, two ninths of a unit.
pub const PLAYER_CAMERA_WATER_CLEARANCE: f32 = 0.222_222_22;

/// Camera subject classification from its registered liquid surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerCameraLiquidState {
    /// The subject registration has no admitted liquid surface.
    Absent,
    /// The subject's top reaches above the native interface clearance.
    Surface {
        /// Surface height minus subject origin, rounded at the caller's store.
        depth: f32,
    },
    /// The subject's top lies below the native interface clearance.
    Submerged {
        /// Surface height minus subject origin, rounded at the caller's store.
        depth: f32,
    },
}

impl PlayerCameraLiquidState {
    /// Samples 6049C0 using the unit height and 77F1E0 registration result.
    ///
    /// # Errors
    /// Rejects non-finite inputs instead of manufacturing a water state.
    pub fn sample(
        subject_z: f32,
        subject_height: f32,
        registered_surface: Option<f32>,
    ) -> Result<Self, PlayerCameraLiquidStateError> {
        let Some(surface) = registered_surface else {
            return Ok(Self::Absent);
        };
        if !subject_z.is_finite() || !subject_height.is_finite() || !surface.is_finite() {
            return Err(PlayerCameraLiquidStateError);
        }
        // The comparison precedes the caller's float store of the depth.
        let depth = f64::from(surface) - f64::from(subject_z);
        if depth <= f64::from(subject_height) - f64::from(PLAYER_CAMERA_WATER_CLEARANCE) {
            Ok(Self::Surface {
                depth: depth as f32,
            })
        } else {
            Ok(Self::Submerged {
                depth: depth as f32,
            })
        }
    }
}

/// A non-finite subject, height, or registered liquid surface.
#[derive(Debug, Error)]
#[error("camera subject liquid state contains a non-finite input")]
pub struct PlayerCameraLiquidStateError;

/// Invalid camera-interface inputs or rejected owning-scene geometry.
#[derive(Debug, Error)]
pub enum PlayerCameraWaterInterfaceError<E> {
    /// A supplied camera vector or orbit distance is non-finite.
    #[error("camera water-interface input is invalid")]
    InvalidInput,
    /// A provider returned a non-finite contact or invalid obstruction distance.
    #[error("camera water-interface provider result is invalid")]
    InvalidResult,
    /// A scene provider could not resolve the requested geometry.
    #[error("camera water-interface scene query failed")]
    Scene(#[source] E),
}

/// Applies 6061D0 after the primary camera obstruction and distance feedback.
///
/// A positive orbit distance probes the vertical segment around the eye using
/// only water geometry (`0x20000`), independently of `cameraWaterCollision`.
/// A contact preserves the eye's original side; equality chooses below. Only
/// a contact invokes the second, solid-only volume test (`0x100171`). Its
/// obstruction result reconstructs the eye from the original camera forward.
/// This stage does not change the camera's orientation or zoom feedback.
///
/// # Errors
/// Rejects invalid inputs, provider results, or owning-scene failures.
pub fn resolve_player_camera_water_interface<E>(
    eye: Vec3,
    pivot: Vec3,
    forward: Vec3,
    orbit_distance: f32,
    mut water_trace: impl FnMut(Vec3, Vec3) -> Result<Option<Vec3>, E>,
    mut solid_volume: impl FnMut(f32, Vec3, Vec3) -> Result<Option<f32>, E>,
) -> Result<Vec3, PlayerCameraWaterInterfaceError<E>> {
    if !eye.is_finite() || !pivot.is_finite() || !forward.is_finite() || !orbit_distance.is_finite()
    {
        return Err(PlayerCameraWaterInterfaceError::InvalidInput);
    }
    if orbit_distance <= 0.0 {
        return Ok(eye);
    }
    let start = Vec3::new(eye.x, eye.y, eye.z + PLAYER_CAMERA_WATER_CLEARANCE);
    let end = Vec3::new(eye.x, eye.y, eye.z - PLAYER_CAMERA_WATER_CLEARANCE);
    let Some(mut contact) =
        water_trace(start, end).map_err(PlayerCameraWaterInterfaceError::Scene)?
    else {
        return Ok(eye);
    };
    if !contact.is_finite() {
        return Err(PlayerCameraWaterInterfaceError::InvalidResult);
    }
    contact.z = if eye.z <= contact.z {
        contact.z - PLAYER_CAMERA_WATER_CLEARANCE
    } else {
        contact.z + PLAYER_CAMERA_WATER_CLEARANCE
    };
    let Some(distance) = solid_volume(orbit_distance, contact, pivot)
        .map_err(PlayerCameraWaterInterfaceError::Scene)?
    else {
        return Ok(contact);
    };
    if !distance.is_finite() || distance < 0.0 || distance > orbit_distance {
        return Err(PlayerCameraWaterInterfaceError::InvalidResult);
    }
    Ok((pivot.as_dvec3() - forward.as_dvec3() * f64::from(distance)).as_vec3())
}
