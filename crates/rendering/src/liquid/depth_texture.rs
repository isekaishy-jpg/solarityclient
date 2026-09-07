//! Generated water depth textures from native 8A2BF0 and 8A2AC0.

/// Stock procedural texture families named by `LiquidType.dbc`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiquidDepthTextureKind {
    /// `proceduralRiverDepthTex`, including the opaque, darkened last row.
    River,
    /// `proceduralOceanDepthTex`, with the ordinary depth gradient throughout.
    Ocean,
    /// `proceduralWmoWaterTex`, split between the environment color and white.
    WorldModel,
}

/// One 8-by-64, linear-color RGBA8 procedural liquid image.
#[derive(Debug, Eq, PartialEq)]
pub struct LiquidDepthTexture {
    pixels_rgba: Box<[u8; Self::BYTE_SIZE]>,
}

impl LiquidDepthTexture {
    /// Number of texels across the procedural image.
    pub const WIDTH: u32 = 8;
    /// Number of depth samples down the procedural image.
    pub const HEIGHT: u32 = 64;
    /// Size of its tightly packed RGBA8 payload.
    pub const BYTE_SIZE: usize = 8 * 64 * 4;

    /// Generates the image from the stock environment's packed shallow/deep colors.
    ///
    /// Colors use `0xAARRGGBB`; their alpha bytes are ignored. The separate alpha
    /// pair has already crossed stock's normalized-float-to-byte boundary.
    /// WMO water consumes the deep color for its first four columns.
    #[must_use]
    pub fn prepare(kind: LiquidDepthTextureKind, colors: [u32; 2], alphas: [u8; 2]) -> Self {
        let start = rgba(colors[0], alphas[0]);
        let end = rgba(colors[1], alphas[1]);
        let mut pixels_rgba = Box::new([0; Self::BYTE_SIZE]);
        for row in 0..64 {
            // Native uses 8.8 accumulators and divides by 64, so the final
            // ordinary row is 63/64 of the way to the deep endpoint.
            let gradient: [u8; 4] = std::array::from_fn(|channel| {
                let shallow = i32::from(start[channel]);
                let deep = i32::from(end[channel]);
                ((shallow * 256 + (deep - shallow) * 4 * row as i32) >> 8) as u8
            });
            let gradient = if kind == LiquidDepthTextureKind::River && row == 63 {
                darken_river_tail(gradient)
            } else {
                gradient
            };
            for column in 0..8 {
                let pixel = match kind {
                    LiquidDepthTextureKind::River | LiquidDepthTextureKind::Ocean => gradient,
                    LiquidDepthTextureKind::WorldModel => rgba(
                        if column < 4 { colors[1] } else { 0x00ff_ffff },
                        gradient[3],
                    ),
                };
                let offset = (row * 8 + column) * 4;
                pixels_rgba[offset..offset + 4].copy_from_slice(&pixel);
            }
        }
        Self { pixels_rgba }
    }

    /// Returns tightly packed rows suitable for a linear RGBA8 sampled image.
    #[must_use]
    pub fn pixels_rgba(&self) -> &[u8; Self::BYTE_SIZE] {
        &self.pixels_rgba
    }
}

/// Extracts the environment word without applying color-space conversion.
fn rgba(color: u32, alpha: u8) -> [u8; 4] {
    [(color >> 16) as u8, (color >> 8) as u8, color as u8, alpha]
}

/// Preserves 982970 -> 984F60 -> 985030 -> 9851A0, including HSV float stores.
fn darken_river_tail(pixel: [u8; 4]) -> [u8; 4] {
    let rgb = [pixel[0], pixel[1], pixel[2]].map(|channel| f32::from(channel) * 0.003_921_569_f32);
    let maximum = rgb[0].max(rgb[1]).max(rgb[2]);
    let minimum = rgb[0].min(rgb[1]).min(rgb[2]);
    let value = maximum * 0.9_f32;
    if maximum == minimum {
        let channel = pack_channel(value);
        return [channel, channel, channel, 255];
    }
    let range = f64::from(maximum) - f64::from(minimum);
    let saturation = (range / f64::from(maximum)) as f32;
    let hue = if maximum == rgb[0] {
        ((f64::from(rgb[1]) - f64::from(rgb[2])) / range) as f32
    } else if maximum == rgb[1] {
        ((f64::from(rgb[2]) - f64::from(rgb[0])) / range + 2.0) as f32
    } else {
        ((f64::from(rgb[0]) - f64::from(rgb[1])) / range + 4.0) as f32
    };
    let mut hue = hue * 60.0;
    if hue < 0.0 {
        hue += 360.0;
    }
    if hue >= 360.0 {
        hue -= 360.0;
    }
    let sector_phase = hue * 0.016_666_668_f32;
    let sector = (f64::from(sector_phase) - 0.5).round_ties_even().min(5.0) as u32;
    let phase = f64::from(sector_phase) - f64::from(sector);
    let saturation = f64::from(saturation.min(1.0));
    let value_extended = f64::from(value);
    let low = ((1.0 - saturation) * value_extended) as f32;
    let descending = ((1.0 - saturation * phase) * value_extended) as f32;
    let ascending = ((1.0 - (1.0 - phase) * saturation) * value_extended) as f32;
    let rgb = match sector {
        0 => [value, ascending, low],
        1 => [descending, value, low],
        2 => [low, value, ascending],
        3 => [low, descending, value],
        4 => [ascending, low, value],
        5.. => [value, low, descending],
    };
    [
        pack_channel(rgb[0]),
        pack_channel(rgb[1]),
        pack_channel(rgb[2]),
        255,
    ]
}

/// Native FISTP rounds to nearest even before the low byte is stored.
fn pack_channel(value: f32) -> u8 {
    (f64::from(value) * 255.0).round_ties_even() as i32 as u8
}
