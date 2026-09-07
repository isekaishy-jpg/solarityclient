//! World-space ripple projection from native 79D5E0 and 7E2D60.

use glam::{Mat4, Vec3};
use thiserror::Error;

/// A ripple cannot produce the nondegenerate projection required by 7E2D60.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error(
    "water ripple projection requires finite position and yaw and nondegenerate positive radius"
)]
pub struct WaterRippleProjectionError;

/// Projects frozen liquid triangles into the growing ripple texture rectangle.
///
/// `stored_yaw` is the negated emission yaw retained by 79D180. Bounds are
/// rounded before deriving their midpoint and scale, including at large world
/// coordinates. Matrix products preserve 4C1F00's wide intermediate arithmetic.
///
/// # Errors
/// Rejects nonfinite inputs and bounds narrower than native 7E2D60's 0.001
/// threshold, for which the original function leaves its output untouched.
pub fn water_ripple_surface_transform(
    position: Vec3,
    radius: f32,
    stored_yaw: f32,
) -> Result<Mat4, WaterRippleProjectionError> {
    if !position.is_finite() || !radius.is_finite() || radius <= 0. || !stored_yaw.is_finite() {
        return Err(WaterRippleProjectionError);
    }
    let minimum = position - Vec3::new(radius, radius, 1.);
    let maximum = position + Vec3::new(radius, radius, 1.);
    let widths = [
        f64::from(maximum.x) - f64::from(minimum.x),
        f64::from(maximum.y) - f64::from(minimum.y),
    ];
    if !minimum.is_finite()
        || !maximum.is_finite()
        || widths.iter().any(|width| *width < f64::from(0.001_f32))
    {
        return Err(WaterRippleProjectionError);
    }
    let midpoint = Vec3::from_array(std::array::from_fn(|index| {
        ((f64::from(minimum[index]) + f64::from(maximum[index])) * 0.5) as f32
    }));
    let translation = Mat4::from_translation(-midpoint);
    let scale = Mat4::from_scale(Vec3::new(
        (1. / widths[0]) as f32,
        (1. / widths[1]) as f32,
        1.,
    ));
    // 9F1FF4 is the authored single-precision -pi/2 rotation. Preserve its
    // residual cosine rather than replacing it with an exact axis exchange.
    let axis_rotation = rotation(f32::from_bits(0xbfc9_0fdb));
    let translated = multiply(translation, scale);
    let oriented = multiply(translated, axis_rotation);
    let mut projection = multiply(oriented, rotation(stored_yaw));
    projection.w_axis.x += 0.5;
    projection.w_axis.y += 0.5;
    Ok(projection)
}

/// Matches the f32 stores following native 4C3290's x87 trigonometry.
fn rotation(yaw: f32) -> Mat4 {
    let (sine, cosine) = f64::from(yaw).sin_cos();
    let (sine, cosine) = (sine as f32, cosine as f32);
    Mat4::from_cols_array(&[
        cosine, sine, 0., 0., -sine, cosine, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ])
}

/// Applies a native row-vector product using column-vector storage. Each dot
/// product rounds only at its final float store, as 4C1F00 does.
fn multiply(left: Mat4, right: Mat4) -> Mat4 {
    let left = left.to_cols_array();
    let right = right.to_cols_array();
    Mat4::from_cols_array(&std::array::from_fn(|index| {
        let column = index / 4;
        let row = index % 4;
        (0..4)
            .map(|component| {
                f64::from(left[column * 4 + component]) * f64::from(right[component * 4 + row])
            })
            .sum::<f64>() as f32
    }))
}
