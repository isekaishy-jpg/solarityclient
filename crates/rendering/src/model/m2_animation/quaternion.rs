//! Native M2 quaternion interpolation and matrix composition without unit assumptions.

use glam::{Mat4, Quat, Vec4};

/// `0x00982630` lerps components without a hemisphere flip, then applies
/// `0x00982570`'s bounded polynomial inverse-length approximation.
pub(super) fn interpolate_quaternion(first: Quat, second: Quat, amount: f32) -> Quat {
    let first = first.to_array();
    let second = second.to_array();
    let values: [f32; 4] = std::array::from_fn(|index| {
        (f64::from(first[index])
            + (f64::from(second[index]) - f64::from(first[index])) * f64::from(amount))
            as f32
    });
    // These are the exact constants at 0x00AA2E5C..6C. Keep x87 intermediates
    // wider than the stored values, rounding only the final components.
    let base = f64::from(f32::from_bits(0x3f82_be62));
    let slope = f64::from(f32::from_bits(0x3f08_52f8));
    let center = f64::from(f32::from_bits(0x3f75_8559));
    let third_threshold = f64::from(f32::from_bits(0x3f26_f151));
    let second_threshold = f64::from(f32::from_bits(0x3f6a_4b55));
    let length_squared = values
        .into_iter()
        .map(|value| f64::from(value).powi(2))
        .sum::<f64>();
    let mut factor = base - (length_squared - center) * slope;
    if length_squared <= second_threshold {
        factor *= base - (factor * factor * length_squared - center) * slope;
        if length_squared <= third_threshold {
            factor *= base - (factor * factor * length_squared - center) * slope;
        }
    }
    Quat::from_array(values.map(|value| (f64::from(value) * factor) as f32))
}

/// 0x00982460 takes the shortest arc and retains the first quaternion when the
/// sine is below the authored 2^-21 threshold. Extended intermediates avoid
/// prematurely rounding the dot product of compressed input quaternions.
pub(super) fn blend_quaternion(primary: Quat, previous: Quat, weight: f32) -> Quat {
    let primary = primary.to_array().map(f64::from);
    let previous = previous.to_array().map(f64::from);
    let dot = primary
        .iter()
        .zip(previous)
        .map(|(a, b)| a * b)
        .sum::<f64>();
    let sine = (1.0 - dot * dot).abs().sqrt();
    if sine < 1.0 / 2_097_152.0 {
        return Quat::from_array(primary.map(|value| value as f32));
    }
    let angle = sine.atan2(dot.abs());
    let first_weight = ((1.0 - f64::from(weight)) * angle).sin() / sine;
    let previous_weight =
        (f64::from(weight) * angle).sin() / sine * if dot < 0.0 { -1.0 } else { 1.0 };
    Quat::from_array(std::array::from_fn(|index| {
        (primary[index] * first_weight + previous[index] * previous_weight) as f32
    }))
}

/// `0x004C1C40` builds the rotation matrix directly from retained components.
/// Stock step keys and approximate normalization need not be unit quaternions.
pub(super) fn quaternion_matrix(rotation: Quat) -> Mat4 {
    let [x, y, z, w] = rotation.to_array().map(f64::from);
    let column = |a: f64, b: f64, c: f64| Vec4::new(a as f32, b as f32, c as f32, 0.0);
    Mat4::from_cols(
        column(
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + w * z),
            2.0 * (x * z - w * y),
        ),
        column(
            2.0 * (x * y - w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + w * x),
        ),
        column(
            2.0 * (x * z + w * y),
            2.0 * (y * z - w * x),
            1.0 - 2.0 * (x * x + y * y),
        ),
        Vec4::W,
    )
}
