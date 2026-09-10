//! Native cosine-smoothed value noise and opacity lookup tables.

mod constants;
use constants::PERMUTATION;

/// Seeded native values and fixed smoothing/opacity lookup tables.
pub(super) struct CloudNoise {
    pub(super) values: [f32; 256],
    pub(super) smooth: [f32; 256],
    pub(super) grain: [u8; 256],
}

/// One row's Y/Z interpolation state, shared by its 128 column samples.
pub(super) struct CloudNoiseRow<'a> {
    noise: &'a CloudNoise,
    phase: u16,
    fz: f64,
    octaves: [CloudNoiseOctave; 4],
}

/// The four permuted Y/Z corners and interpolation weight for one octave.
struct CloudNoiseOctave {
    increment: u16,
    fy: f64,
    bases: [usize; 4],
    /// Neighboring columns remain in the same noise cell until X's high byte changes.
    sampled_ix: Option<usize>,
    /// Native f32 left values and corner differences, promoted for interpolation.
    corners: [(f64, f64); 4],
}

/// Wraps the native permutation index before reading its byte value.
fn permute(index: usize) -> usize {
    usize::from(PERMUTATION[index & 255])
}

impl CloudNoise {
    /// Reproduces the native CRT sequence and cosine/grain table rounding.
    pub(super) fn new(mut seed: u32) -> Self {
        let values = std::array::from_fn(|_| {
            seed = seed.wrapping_mul(214013).wrapping_add(2531011);
            let value = f64::from((seed >> 16) & 32767) * f64::from(1.0_f32 / 32767.0);
            (1.0 - (value + value)) as f32
        });
        let smooth = std::array::from_fn(|i| {
            ((1.0 - (i as f64 * f64::from(std::f32::consts::PI / 256.0)).cos()) * 0.5) as f32
        });
        // 7F2790 constructs the grain curve after the registered 0.6 density.
        let step = 154.0 / 256.0;
        let grain = std::array::from_fn(|i| {
            (255.0 - f64::from(0.96_f32).powf(i as f64 * step) * 255.0) as u8
        });
        Self {
            values,
            smooth,
            grain,
        }
    }

    /// Hoists only the column-independent work in the native four-octave noise.
    pub(super) fn row(&self, y: usize, phase: u16) -> CloudNoiseRow<'_> {
        let iz = usize::from(phase >> 8);
        let octaves = [16_u16, 32, 64, 128].map(|increment| {
            let py = (y as u16).wrapping_mul(increment);
            let iy = usize::from(py >> 8);
            CloudNoiseOctave {
                increment,
                fy: f64::from(self.smooth[usize::from(py & 255)]),
                bases: [
                    permute(permute(iz) + iy),
                    permute(permute(iz) + iy + 1),
                    permute(permute(iz + 1) + iy),
                    permute(permute(iz + 1) + iy + 1),
                ],
                sampled_ix: None,
                corners: [(0.0, 0.0); 4],
            }
        });
        CloudNoiseRow {
            noise: self,
            phase,
            fz: f64::from(self.smooth[usize::from(phase & 255)]),
            octaves,
        }
    }
}

impl CloudNoiseRow<'_> {
    /// Refreshes corners only when X changes cells, retaining native f32
    /// differences and f64 interpolation order for every column.
    pub(super) fn sample(&mut self, x: usize) -> (f64, f64) {
        let mut sum = 0.0;
        let mut partial = 0.0;
        for (octave, state) in self.octaves.iter_mut().enumerate() {
            let px = self
                .phase
                .wrapping_add((x as u16).wrapping_mul(state.increment));
            let fx = f64::from(self.noise.smooth[usize::from(px & 255)]);
            let ix = usize::from(px >> 8);
            // Four octaves advance by 16/32/64/128, so each cell supplies
            // multiple columns. Reuse only its corners; fx still changes per pixel.
            if state.sampled_ix != Some(ix) {
                state.corners = state.bases.map(|base| {
                    let a = self.noise.values[permute(base + ix)];
                    let b = self.noise.values[permute(base + ix + 1)];
                    (f64::from(a), f64::from(b - a))
                });
                state.sampled_ix = Some(ix);
            }
            let [a, b, c, d] = state.corners.map(|(a, difference)| a + difference * fx);
            let low = a + (b - a) * state.fy;
            let high = c + (d - c) * state.fy;
            sum += (low + (high - low) * self.fz) / f64::from(1_u32 << octave);
            if octave == 2 {
                partial = sum;
            }
        }
        (sum, partial)
    }
}
