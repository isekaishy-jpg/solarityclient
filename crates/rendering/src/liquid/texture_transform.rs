//! Authored liquid texture matrices from 8A5590, 8A6090, and 8A34B0.

use glam::{Mat4, Vec3};
use thiserror::Error;

/// An authored scroll rate produces an unusable native clock divisor.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
#[error("liquid scroll rate {rate} produces a zero clock period")]
pub struct LiquidScrollError {
    rate: f32,
}

/// Builds native water's rotation followed by uniform basis scaling.
///
/// The authored angle is multiplied by the original constant at 9ED910 before
/// native 4C3290 consumes it as radians. This is deliberately not a conventional
/// degrees-to-radians conversion. Depth scale is applied by a separate matrix.
#[must_use]
pub fn liquid_water_surface_transform(scale: f32, angle: f32) -> Mat4 {
    let radians = (f64::from(angle) * f64::from(57.295_78_f32)) as f32;
    let (sine, cosine) = f64::from(radians).sin_cos();
    let sine = sine as f32;
    let cosine = cosine as f32;
    Mat4::from_cols_array(&[
        cosine * scale,
        sine * scale,
        0.0,
        0.0,
        -sine * scale,
        cosine * scale,
        0.0,
        0.0,
        0.0,
        0.0,
        scale,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ])
}

/// Samples native magma's two independent scrolling axes at the engine clock.
///
/// Native 8A34B0 truncates its millisecond period, then divides the clock
/// remainder by that period. 4C1BF0 scales the basis while retaining translation.
///
/// # Errors
/// Returns [`LiquidScrollError`] when a nonzero rate truncates to a zero divisor.
pub fn liquid_magma_surface_transform(
    rates: [f32; 2],
    scale: f32,
    time_ms: u32,
) -> Result<Mat4, LiquidScrollError> {
    let mut translation = [0.0; 2];
    for (coordinate, rate) in translation.iter_mut().zip(rates) {
        if rate == 0.0 {
            continue;
        }
        // 8A34E8 explicitly selects x87 truncation before signed 64-bit FISTP.
        let period = (1000.0 / f64::from(rate)) as i64 as u32;
        if period == 0 {
            return Err(LiquidScrollError { rate });
        }
        // FILD and the unsigned correction remain extended until the ratio's
        // final float store at 8A35E7/8A35F2.
        *coordinate = (f64::from(time_ms % period) / f64::from(period)) as f32;
    }
    let mut matrix = Mat4::from_scale(Vec3::splat(scale));
    matrix.w_axis.x = translation[0];
    matrix.w_axis.y = translation[1];
    Ok(matrix)
}
