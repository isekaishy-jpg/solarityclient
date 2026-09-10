//! Original generated V8U8 displacement and its animated texture matrix.

/// Generates 8C2920's separable 128x128 texture without random state.
pub(super) fn texture() -> Vec<u8> {
    let axis: [u8; 128] = std::array::from_fn(|index| {
        let phase = (index as f64 / 128.
            * f64::from(f32::from_bits(0x40c9_0fdb))
            * f64::from(f32::from_bits(0x3ea2_f983))
            - 0.5) as f32;
        // 5FE800 truncates and subtracts one on its non-positive branch.
        let period = phase as i32 - i32::from(phase <= 0.);
        let fraction = f64::from((f64::from(phase) - f64::from(period)) as f32);
        let value = 1. - (6. - 4. * fraction) * fraction * fraction;
        let signed = if period & 1 != 0 { -value } else { value };
        ((signed * 128.).clamp(-128., 127.) as i8) as u8
    });
    let mut pixels = Vec::with_capacity(128 * 128 * 2);
    for y in axis {
        for x in axis {
            pixels.extend([x, y]);
        }
    }
    pixels
}

/// Rows of 8C2350's texture matrix, before 8C0590's half-texel coordinates.
pub(super) fn transform(extent: (u32, u32), milliseconds: u32) -> [[f32; 4]; 2] {
    let scale_x = extent.0 as f32 / 128.;
    let scale_y = (f64::from(extent.1) * f64::from(0.88_f32) / 128.) as f32;
    let (sin, cos) = f64::from(f32::from_bits(0x3e32_b8c3)).sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    [
        [
            scale_x * cos,
            -scale_x * sin,
            (milliseconds % 3174) as f32 / 3174.,
            0.,
        ],
        [
            scale_y * sin,
            scale_y * cos,
            (milliseconds % 2805) as f32 / 2805.,
            0.,
        ],
    ]
}

/// Adapts the native vertex UVs to Vulkan's half-pixel fragment centers.
pub(super) fn fragment_transform(extent: (u32, u32), milliseconds: u32) -> [[f32; 4]; 2] {
    let mut rows = transform(extent, milliseconds);
    let dx = 0.5 / 128. - 0.5 / f64::from(extent.0);
    let dy = 0.5 / 128. - 0.5 / f64::from(extent.1);
    for row in &mut rows {
        row[2] = (f64::from(row[0]) * dx + f64::from(row[1]) * dy + f64::from(row[2])) as f32;
    }
    rows
}

#[cfg(test)]
#[path = "../../../tests/unit/world_glow_wave.rs"]
mod tests;
