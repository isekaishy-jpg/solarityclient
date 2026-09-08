//! Native cosine-smoothed value noise and opacity lookup tables.

mod constants;
use constants::PERMUTATION;

pub(super) struct CloudNoise {
    pub(super) values: [f32; 256],
    pub(super) smooth: [f32; 256],
    pub(super) grain: [u8; 256],
}

impl CloudNoise {
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

    pub(super) fn sample(&self, x: usize, y: usize, phase: u16) -> (f64, f64) {
        let mut sum = 0.0;
        let mut partial = 0.0;
        for (octave, increment) in [16_u16, 32, 64, 128].into_iter().enumerate() {
            let px = phase.wrapping_add((x as u16).wrapping_mul(increment));
            let py = (y as u16).wrapping_mul(increment);
            let [fx, fy, fz] =
                [px, py, phase].map(|p| f64::from(self.smooth[usize::from(p & 255)]));
            let ix = usize::from(px >> 8);
            let iy = usize::from(py >> 8);
            let iz = usize::from(phase >> 8);
            let permute = |i: usize| usize::from(PERMUTATION[i & 255]);
            let across = |yy: usize, zz: usize| {
                let base = permute(permute(zz) + yy);
                let a = self.values[permute(base + ix)];
                let b = self.values[permute(base + ix + 1)];
                f64::from(a) + f64::from(b - a) * fx
            };
            let a = across(iy, iz);
            let b = across(iy + 1, iz);
            let c = across(iy, iz + 1);
            let d = across(iy + 1, iz + 1);
            let low = a + (b - a) * fy;
            let high = c + (d - c) * fy;
            sum += (low + (high - low) * fz) / f64::from(1_u32 << octave);
            if octave == 2 {
                partial = sum;
            }
        }
        (sum, partial)
    }
}
