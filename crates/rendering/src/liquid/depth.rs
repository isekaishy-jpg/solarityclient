//! Native 79E3C0 depth-byte lookup tables consumed by liquid vertices.

/// The two depth coordinate banks selected by `LiquidType` integer parameter zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiquidDepthCoordinates {
    /// River depth reaches the last texel after the stock shallow-water range.
    River,
    /// Ocean depth spans the full authored byte range.
    Ocean,
}

impl LiquidDepthCoordinates {
    /// Converts an authored byte to the procedural depth texture's vertical UV.
    ///
    /// `79E3C0` retains extended intermediates until the final float table store;
    /// the constants below preserve the original single-precision input bits.
    #[must_use]
    pub fn coordinate(self, depth: u8) -> f32 {
        let depth = f64::from(depth);
        match self {
            Self::River => {
                let distance = depth * f64::from(0.111_111_11_f32);
                if distance <= f64::from(4.666_666_5_f32) {
                    (distance * f64::from(0.214_285_72_f32)) as f32
                } else {
                    1.0
                }
            }
            Self::Ocean => {
                let scale = f64::from(0.583_333_3_f32);
                (depth * scale * (1.0 / (255.0 * scale))).min(1.0) as f32
            }
        }
    }
}
