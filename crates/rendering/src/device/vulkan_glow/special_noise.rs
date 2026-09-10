//! Deterministic 7E8E40 texture generation, shared by the build and native tests.

/// Generates the fixed 256x256 white-RGB, value-noise-alpha texture.
pub fn texture() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256 {
        for x in 0..256 {
            let value = fractal(x as f32 * 0.25, y as f32 * 0.25);
            let alpha = ((value + 3.) * 0.25 * 255. + 0.5) as u32 as u8;
            pixels.extend([255, 255, 255, alpha]);
        }
    }
    pixels
}

fn fractal(x: f32, y: f32) -> f64 {
    let mut amplitude = 1_f32;
    let mut sum = 0_f64;
    for octave in 0..5 {
        let frequency = (1 << octave) as f32;
        // 985580 spills the previous sum, but retains the last return in x87.
        sum = interpolated(x * frequency, y * frequency) * f64::from(amplitude)
            + f64::from(sum as f32);
        amplitude *= 0.5;
    }
    sum
}

fn interpolated(x: f32, y: f32) -> f64 {
    let floor_x = x.floor();
    let floor_y = y.floor();
    let (x0, y0) = (floor_x as u32, floor_y as u32);
    let fraction_x = f64::from(x - floor_x);
    let a = f64::from(smoothed(x0, y0) as f32);
    let b = f64::from(smoothed(x0, y0.wrapping_add(1)) as f32);
    let lower = f64::from(((smoothed(x0.wrapping_add(1), y0) - a) * fraction_x + a) as f32);
    let upper = (smoothed(x0.wrapping_add(1), y0.wrapping_add(1)) - b) * fraction_x + b;
    lower + (f64::from(y) - f64::from(floor_y)) * (upper - lower)
}

fn smoothed(x: u32, y: u32) -> f64 {
    let center = y.wrapping_mul(57).wrapping_add(x);
    let corners = [-58_i32, -56, 56, 58]
        .map(|offset| hash(center.wrapping_add_signed(offset)))
        .into_iter()
        .sum::<f64>();
    let sides = [-1_i32, 1, -57, 57]
        .map(|offset| hash(center.wrapping_add_signed(offset)))
        .into_iter()
        .sum::<f64>();
    hash(center) * 0.25 + corners * 0.0625 + sides * 0.125
}

fn hash(value: u32) -> f64 {
    let value = value ^ value.wrapping_shl(13);
    let polynomial = value
        .wrapping_mul(value)
        .wrapping_mul(0x3d73)
        .wrapping_add(0xc0ae5)
        .wrapping_mul(value)
        .wrapping_add(0xd208dd03)
        & 0x7fffffff;
    1. - f64::from(polynomial) * 2_f64.powi(-30)
}
