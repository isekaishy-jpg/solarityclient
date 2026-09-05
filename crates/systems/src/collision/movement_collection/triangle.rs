//! Stock terrain/WMO plane preparation and authored M2 transforms.

use super::{MovementCollectionError, MovementCollisionBounds};
use crate::collision::MovementCollisionTriangle;
use glam::{Mat4, Vec3};

/// x87 keeps vertex differences wide until the three cross-component stores.
pub(in crate::collision) fn calculated_triangle(
    vertices: [Vec3; 3],
) -> Result<MovementCollisionTriangle, MovementCollectionError> {
    let first = vertices[1].as_dvec3() - vertices[0].as_dvec3();
    let second = vertices[2].as_dvec3() - vertices[0].as_dvec3();
    let cross = first.cross(second).as_vec3();
    let length_squared = cross.as_dvec3().length_squared();
    let reciprocal = solarity_cpu::reciprocal_sqrt_estimate(length_squared as f32)
        .map_or_else(|| length_squared.sqrt().recip(), f64::from);
    let normal = (cross.as_dvec3() * reciprocal).as_vec3();
    Ok(MovementCollisionTriangle::with_normal(vertices, normal)?)
}

/// WMO transforms the retained local cross product before normalizing it.
pub(super) fn world_model_triangle(
    vertices: [Vec3; 3],
    transform: Mat4,
) -> Result<MovementCollisionTriangle, MovementCollectionError> {
    let first = vertices[1].as_dvec3() - vertices[0].as_dvec3();
    let second = vertices[2].as_dvec3() - vertices[0].as_dvec3();
    let cross = first.cross(second);
    let normal = Vec3::from_array(std::array::from_fn(|axis| {
        (cross.x * f64::from(transform.x_axis[axis])
            + cross.y * f64::from(transform.y_axis[axis])
            + cross.z * f64::from(transform.z_axis[axis])) as f32
    }));
    let length_squared = normal.as_dvec3().length_squared() as f32;
    let normal = if length_squared == 0.0 {
        Vec3::Z
    } else {
        let reciprocal = solarity_cpu::reciprocal_sqrt_estimate(length_squared)
            .map_or_else(|| f64::from(length_squared).sqrt().recip(), f64::from);
        (normal.as_dvec3() * reciprocal).as_vec3()
    };
    Ok(MovementCollisionTriangle::with_normal(
        vertices.map(|v| transform_point(transform, v)),
        normal,
    )?)
}

/// `0x007A55E0` recenters the box before accumulating rotated axis intervals.
pub(super) fn transformed_query(
    bounds: MovementCollisionBounds,
    inverse: Mat4,
) -> Result<MovementCollisionBounds, MovementCollectionError> {
    let center = ((bounds.minimum.as_dvec3() + bounds.maximum.as_dvec3()) * 0.5).as_vec3();
    let minimum = bounds.minimum - center;
    let maximum = bounds.maximum - center;
    let local_center = transform_point(inverse, center);
    let mut local_minimum = Vec3::ZERO;
    let mut local_maximum = Vec3::ZERO;
    for (source_axis, rotation) in [inverse.x_axis, inverse.y_axis, inverse.z_axis]
        .into_iter()
        .enumerate()
    {
        for target_axis in 0..3 {
            let first = f64::from(minimum[source_axis]) * f64::from(rotation[target_axis]);
            let second = f64::from(maximum[source_axis]) * f64::from(rotation[target_axis]);
            local_minimum[target_axis] =
                (f64::from(local_minimum[target_axis]) + first.min(second)) as f32;
            local_maximum[target_axis] =
                (f64::from(local_maximum[target_axis]) + first.max(second)) as f32;
        }
    }
    MovementCollisionBounds::new(local_minimum + local_center, local_maximum + local_center)
}

/// Stock's scalar matrix-point path retains products until each float store.
pub(in crate::collision) fn transform_point(transform: Mat4, point: Vec3) -> Vec3 {
    Vec3::from_array(std::array::from_fn(|axis| {
        (f64::from(point.x) * f64::from(transform.x_axis[axis])
            + f64::from(point.y) * f64::from(transform.y_axis[axis])
            + f64::from(point.z) * f64::from(transform.z_axis[axis])
            + f64::from(transform.w_axis[axis])) as f32
    }))
}
