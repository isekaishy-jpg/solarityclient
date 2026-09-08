//! Native incremental procedural-cloud texture ownership.

mod frame;
mod mesh;
pub use frame::WorldCloudFrame;
mod noise;
mod projection;

pub use mesh::WorldCloudDome;

/// Cloud-specific colors and the projected sun/moon position in texture space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCloudLighting {
    ambient: [f32; 3],
    diffuse: [f32; 3],
    emissive: [f32; 3],
    position: [f32; 3],
    strength: f32,
}

impl WorldCloudLighting {
    /// Stores the original cloud lighting provider's five outputs.
    #[must_use]
    pub const fn new(
        ambient: [f32; 3],
        diffuse: [f32; 3],
        emissive: [f32; 3],
        position: [f32; 3],
        strength: f32,
    ) -> Self {
        Self {
            ambient,
            diffuse,
            emissive,
            position,
            strength,
        }
    }
}

/// Two native texture generations and the current row-by-row noise update.
pub struct WorldClouds {
    noise: noise::CloudNoise,
    pixels: Vec<u8>,
    alpha: Vec<u8>,
    previous: [f32; 128],
    textures: [Vec<u8>; 2],
    current: usize,
    row: usize,
    phase: u16,
    elapsed: f32,
    force: bool,
    revision: u64,
}

impl WorldClouds {
    /// Build 12340's initially selected cloud quality uses 128-square textures.
    pub const SIZE: usize = 128;

    /// Builds the original noise tables from the CRT seed at cloud construction.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self {
            noise: noise::CloudNoise::new(seed),
            pixels: vec![255; 128 * 128 * 4],
            alpha: vec![0; 128 * 128],
            previous: [0.; 128],
            textures: std::array::from_fn(|_| vec![255; 128 * 128 * 4]),
            current: 0,
            row: 0,
            phase: 0,
            elapsed: 0.,
            force: true,
            revision: 0,
        }
    }

    /// Requests the native complete initial refresh rather than an eight-row slice.
    pub fn invalidate(&mut self) {
        self.force = true;
    }

    /// Advances one native frame: eight rows, or the full image after invalidation.
    pub fn update(&mut self, elapsed_seconds: f32, density: f32, lighting: WorldCloudLighting) {
        self.elapsed += elapsed_seconds;
        let start = if self.force { 0 } else { self.row };
        let count = if self.force { Self::SIZE } else { 8 };
        let threshold = ((1.0 - f64::from(density)) * 255.0) as i32 as u8;
        for row in start..start + count {
            let mut previous_pixel = 0.0_f32;
            for column in 0..Self::SIZE {
                let (sum, partial) = self.noise.sample(column, row, self.phase);
                let dx = (f64::from(previous_pixel) - partial) as f32;
                let dy = (f64::from(self.previous[column]) - partial) as f32;
                previous_pixel = partial as f32;
                self.previous[column] = partial as f32;
                let noise_byte = ((sum * 64.0 + 128.0) as f32).round_ties_even() as i32 as u8;
                let alpha = noise_byte
                    .checked_sub(threshold)
                    .map_or(0, |v| self.noise.grain[usize::from(v)]);
                let index = row * Self::SIZE + column;
                self.alpha[index] = alpha;
                if alpha == 0 {
                    // The native first column retains its previous packed color,
                    // even though its independent opacity sample becomes zero.
                    if column > 0 {
                        self.pixels.copy_within(index * 4 - 4..index * 4, index * 4);
                        self.pixels[index * 4 + 3] = 0;
                    }
                } else {
                    self.pixels[index * 4..index * 4 + 4]
                        .copy_from_slice(&shade(column, row, dx, dy, alpha, lighting));
                }
            }
        }
        let target = 1 - self.current;
        let range = start * Self::SIZE * 4..(start + count) * Self::SIZE * 4;
        self.textures[target][range.clone()].copy_from_slice(&self.pixels[range]);
        self.row = start + count;
        if self.row >= Self::SIZE {
            let next_phase = (f64::from(self.elapsed) * 2.0) as i32 as u16;
            if self.phase != next_phase || self.force {
                self.current = target;
                self.revision = self.revision.wrapping_add(1);
            }
            self.phase = next_phase;
            self.row = 0;
        }
        self.force = false;
    }

    /// Returns the currently displayed, complete texture in native BGRA8 order.
    #[must_use]
    pub fn bgra8(&self) -> &[u8] {
        &self.textures[self.current]
    }

    /// Changes only when the original two-texture owner switches the visible bank.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

fn shade(x: usize, y: usize, dx: f32, dy: f32, alpha: u8, light: WorldCloudLighting) -> [u8; 4] {
    let lx = f64::from(light.position[0]) - x as f64;
    let ly = f64::from(light.position[1]) - y as f64;
    let lz = f64::from(light.position[2]);
    let dx = f64::from(dx);
    let dy = f64::from(dy);
    let scale = f64::from(((255 - alpha) >> 1) + 64) * f64::from(1.0_f32 / 255.0);
    let mut rgb = std::array::from_fn::<_, 3, _>(|i| {
        f64::from(light.ambient[i]) * scale + f64::from(light.emissive[i])
    });
    rgb[1] = f64::from(rgb[1] as f32);
    rgb[2] = f64::from(rgb[2] as f32);
    let inverse_length = |value: f32| {
        f64::from(f32::from_bits(
            0x5f39_97bb_u32.wrapping_sub((value.to_bits() >> 1) & 0x3fff_ffff),
        ))
    };
    let direct = inverse_length((ly * ly + lx * lx + lz * lz) as f32)
        * inverse_length((dy * dy + dx * dx + 1.0) as f32)
        * (ly * dy + lx * dx + lz);
    if direct > 0.0 {
        let amount = direct * f64::from(light.strength);
        for (i, value) in rgb.iter_mut().enumerate() {
            *value += f64::from(light.diffuse[i]) * amount;
        }
        rgb[1] = f64::from(rgb[1] as f32);
        rgb[2] = f64::from(rgb[2] as f32);
    }
    let rgb = rgb.map(|v| ((v.min(1.0) * 255.0) as f32).round_ties_even() as i32 as u8);
    [rgb[2], rgb[1], rgb[0], alpha]
}

#[cfg(test)]
#[path = "../../tests/stock_seed/cloud_native.rs"]
mod tests;
