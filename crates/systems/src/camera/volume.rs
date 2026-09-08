//! 6059E0's separate liquid and solid swept camera volumes.

mod math;

use glam::Vec3;
use thiserror::Error;

use crate::MovementCollisionBounds;

const NEAR: f32 = 0.2;
const FAR: f32 = 5000.0;
const MINIMUM: f32 = 0.001;

/// Geometry bank selected by the native camera-volume query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerCameraVolumeKind {
    /// Unexpanded near-plane volume, collecting only liquid surfaces.
    Water,
    /// Near-plane volume expanded by 1.75, collecting ordinary scene geometry.
    Solid,
}

/// A finite native camera box and its world-to-unit-cube clipping transform.
pub struct PlayerCameraVolume {
    corners: [Vec3; 8],
    inverse: Option<[f32; 16]>,
    bounds: MovementCollisionBounds,
}

impl PlayerCameraVolume {
    fn new(corners: [Vec3; 8]) -> Result<Self, PlayerCameraVolumeError> {
        let minimum = corners
            .iter()
            .copied()
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let maximum = corners
            .iter()
            .copied()
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        let bounds = MovementCollisionBounds::new(minimum, maximum)
            .map_err(|_| PlayerCameraVolumeError::InvalidGeometry)?;
        let mut matrix = math::IDENTITY;
        for (column, corner) in [3, 1, 4].into_iter().enumerate() {
            matrix[column * 4..column * 4 + 3]
                .copy_from_slice(&(corners[corner] - corners[0]).to_array());
        }
        let inverse = math::inverse(matrix);
        Ok(Self {
            corners,
            inverse,
            bounds,
        })
    }

    /// Returns the native 984240 corner order, near face followed by swept face.
    #[must_use]
    pub const fn corners(&self) -> &[Vec3; 8] {
        &self.corners
    }

    /// Returns the conservative box used to collect scene triangles.
    #[must_use]
    pub const fn bounds(&self) -> MovementCollisionBounds {
        self.bounds
    }

    /// Clips one world triangle against 791380's six unit-cube planes.
    ///
    /// Returns the greatest surviving Z, the retreat fraction measured from
    /// the camera's near plane toward the subject. Faces need no normal or
    /// winding test at this stage; those belong to geometry admission.
    ///
    /// # Errors
    /// Rejects non-finite authored or transformed triangle coordinates.
    pub fn triangle_retreat(
        &self,
        vertices: [Vec3; 3],
    ) -> Result<Option<f32>, PlayerCameraVolumeError> {
        if vertices.iter().any(|p| !p.is_finite()) {
            return Err(PlayerCameraVolumeError::InvalidGeometry);
        }
        // 791640 returns false for a near-singular box after scene collection.
        let Some(inverse) = self.inverse else {
            return Ok(None);
        };
        let mut polygon = [Vec3::ZERO; 12];
        for (output, vertex) in polygon.iter_mut().zip(vertices) {
            *output = math::point(inverse, vertex - self.corners[0]);
            if !output.is_finite() {
                return Err(PlayerCameraVolumeError::InvalidGeometry);
            }
        }
        let mut count = 3;
        let mut scratch = [Vec3::ZERO; 12];
        for plane in 0..6 {
            let distance = |point: Vec3| {
                if plane & 1 == 0 {
                    point[plane / 2]
                } else {
                    1.0 - point[plane / 2]
                }
            };
            let mut previous = polygon[count - 1];
            let mut previous_distance = distance(previous);
            let mut next_count = 0;
            for &current in &polygon[..count] {
                let current_distance = distance(current);
                let previous_outside = previous_distance.is_sign_negative();
                let current_outside = current_distance.is_sign_negative();
                if previous_outside != current_outside {
                    let difference = previous_distance - current_distance;
                    let fraction =
                        previous_distance / if difference == 0.0 { 1.0 } else { difference };
                    scratch[next_count] = (previous.as_dvec3()
                        + (current.as_dvec3() - previous.as_dvec3()) * f64::from(fraction))
                    .as_vec3();
                    next_count += 1;
                }
                if !current_outside {
                    scratch[next_count] = current;
                    next_count += 1;
                }
                previous = current;
                previous_distance = current_distance;
            }
            if next_count == 0 {
                return Ok(None);
            }
            std::mem::swap(&mut polygon, &mut scratch);
            count = next_count;
        }
        Ok(Some(
            polygon[..count]
                .iter()
                .fold(0.0_f32, |maximum, point| maximum.max(point.z)),
        ))
    }
}

/// Invalid input or a degenerate native camera volume.
#[derive(Debug, Error)]
pub enum PlayerCameraVolumeError {
    /// Camera or triangle coordinates cannot form finite geometry.
    #[error("camera volume geometry is invalid")]
    InvalidGeometry,
    /// Viewport aspect ratio must be positive and finite.
    #[error("camera volume aspect ratio is invalid")]
    InvalidAspectRatio,
    /// A provider returned an invalid unit-cube retreat fraction.
    #[error("camera volume retreat fraction is invalid")]
    InvalidRetreat,
}

/// Camera-volume validation or owning-scene failure.
#[derive(Debug, Error)]
pub enum PlayerCameraVolumeQueryError<E> {
    /// Camera or provider geometry is invalid.
    #[error(transparent)]
    Geometry(#[from] PlayerCameraVolumeError),
    /// The owning scene could not collect or clip geometry.
    #[error("camera volume scene query failed")]
    Scene(#[source] E),
}

/// Resolves 6059E0's camera-volume distance from retained scene geometry.
///
/// The water bank is queried at expansion 1.0 when enabled; solid geometry is
/// queried at expansion 1.75. Both accumulate the greatest unit-cube Z before
/// reducing the supplied orbit distance. A hit remains a hit even at Z zero.
///
/// # Errors
/// Rejects invalid camera inputs, clipping output, or scene-provider failures.
pub fn resolve_player_camera_volume<E>(
    distance: f32,
    eye: Vec3,
    pivot: Vec3,
    aspect_ratio: f32,
    water: bool,
    mut scene: impl FnMut(&PlayerCameraVolume, PlayerCameraVolumeKind) -> Result<Option<f32>, E>,
) -> Result<Option<f32>, PlayerCameraVolumeQueryError<E>> {
    if !distance.is_finite() || !eye.is_finite() || !pivot.is_finite() {
        return Err(PlayerCameraVolumeError::InvalidGeometry.into());
    }
    if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
        return Err(PlayerCameraVolumeError::InvalidAspectRatio.into());
    }
    let span = distance - NEAR;
    if span < MINIMUM {
        return Ok(Some(0.0));
    }
    let delta = pivot.as_dvec3() - eye.as_dvec3();
    let squared = delta.length_squared();
    if squared < f64::from(MINIMUM) {
        return Ok(None);
    }
    let forward = glam::DVec3::new(
        delta.x,
        f64::from(delta.y as f32),
        f64::from(delta.z as f32),
    ) * (1.0 / squared.sqrt());
    let mut up = forward.cross(glam::DVec3::X).as_vec3();
    if up.as_dvec3().length_squared() < f64::from(MINIMUM) {
        up = forward.cross(glam::DVec3::Y).as_vec3();
    }
    up = math::normalize(up);
    let view = math::look_at(eye, (eye.as_dvec3() + forward).as_vec3(), up);
    let projection = math::projection(aspect_ratio, NEAR, FAR);
    let inverse = math::multiply(
        math::inverse(projection).ok_or(PlayerCameraVolumeError::InvalidGeometry)?,
        math::inverse(view).ok_or(PlayerCameraVolumeError::InvalidGeometry)?,
    );
    let near = (-f64::from(projection[14]) / (f64::from(projection[10]) + 1.0)) as f32;
    let base = [(-near, -near), (-near, near), (near, near), (near, -near)]
        .map(|(x, y)| math::vector4(inverse, [x, y, -near, near]));
    let sum = base[0].as_dvec3() + base[1].as_dvec3() + base[2].as_dvec3() + base[3].as_dvec3();
    let center = glam::DVec3::new(sum.x, sum.y, f64::from(sum.z as f32)) * 0.25;
    let sweep = (forward * f64::from(span)).as_vec3();
    let mut maximum = 0.0_f32;
    let mut hit = false;
    for kind in [PlayerCameraVolumeKind::Water, PlayerCameraVolumeKind::Solid] {
        if kind == PlayerCameraVolumeKind::Water && !water {
            continue;
        }
        let expansion = if kind == PlayerCameraVolumeKind::Water {
            1.0
        } else {
            1.75
        };
        let mut corners = [Vec3::ZERO; 8];
        for index in 0..4 {
            let mut relative = base[index].as_dvec3() - center;
            if index == 0 {
                relative.z = f64::from(relative.z as f32);
            }
            relative *= expansion;
            if index == 0 {
                relative.y = f64::from(relative.y as f32);
            }
            corners[index] = (relative + center).as_vec3();
            corners[index + 4] = if index == 3 {
                (relative + center + sweep.as_dvec3()).as_vec3()
            } else {
                corners[index] + sweep
            };
        }
        let volume = PlayerCameraVolume::new(corners)?;
        if let Some(retreat) = scene(&volume, kind).map_err(PlayerCameraVolumeQueryError::Scene)? {
            if !retreat.is_finite() || !(0.0..=1.0).contains(&retreat) {
                return Err(PlayerCameraVolumeError::InvalidRetreat.into());
            }
            maximum = maximum.max(retreat);
            hit = true;
        }
    }
    Ok(hit.then(|| (f64::from(distance) - f64::from(maximum) * f64::from(span)).max(0.0) as f32))
}
