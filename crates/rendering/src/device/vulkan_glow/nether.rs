//! Retained native invisibility noise and activation fade.

use crate::M2ParticleRandom;

/// One native 6x6 distortion mesh's quantized vertex colors and shader inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldNetherFrame {
    pub(super) colors: [u8; 36],
    pub(super) angle: f32,
    pub(super) fade: f32,
}

impl WorldNetherFrame {
    /// Returns the native composition contribution, capped at 0.75.
    #[must_use]
    pub const fn fade(self) -> f32 {
        self.fade
    }
}

/// Invisibility's independent seeded stream survives activation changes.
#[derive(Clone, Debug)]
pub struct WorldNetherState {
    random: M2ParticleRandom,
    banks: [[f32; 36]; 3],
    phase: f32,
    time: f32,
    fade: f32,
}

impl Default for WorldNetherState {
    fn default() -> Self {
        let mut random = M2ParticleRandom::new(0xabcd_ef01);
        let banks = std::array::from_fn(|_| std::array::from_fn(|_| noise(&mut random)));
        Self {
            random,
            banks,
            phase: 0.,
            time: 0.,
            fade: 0.,
        }
    }
}

impl WorldNetherState {
    /// 8C02E0 calls the outgoing owner's 7E8E20 callback, clearing only its fade.
    pub fn reset_fade(&mut self) {
        self.fade = 0.;
    }

    /// Advances one drawn frame using its delta and the saved GX view's first axis.
    /// The caller advances this owner only when its native drawing gates admit it.
    #[must_use]
    pub fn advance(&mut self, delta_seconds: f32, view_axis: [f32; 3]) -> WorldNetherFrame {
        let delta = f64::from(delta_seconds);
        self.time = ((f64::from(self.time) + delta * 2.) % f64::from(std::f32::consts::TAU)) as f32;
        let [mut x, mut y, z] = view_axis.map(f64::from);
        if z.abs() > f64::from(0.0001_f32) {
            let reciprocal = 1. / z;
            x *= reciprocal;
            y *= reciprocal;
        }
        let reciprocal = 1. / (x * x + y * y).sqrt();
        let angle = (x * reciprocal).acos();
        let angle = if ((y * reciprocal) as f32) < 0. {
            f64::from(std::f32::consts::TAU) - angle
        } else {
            angle
        };
        let angle = (angle + f64::from(self.time).cos() * 0.5) as f32;
        if self.phase > 1. {
            self.phase %= 1.;
            self.banks.rotate_left(1);
            self.banks[2] = std::array::from_fn(|_| noise(&mut self.random));
        }
        let phase = f64::from(self.phase);
        let colors = std::array::from_fn(|index| {
            let noise = ((1. - phase) * f64::from(self.banks[0][index])
                + phase * f64::from(self.banks[1][index])) as f32;
            // 8C0DE0 explicitly switches x87 to truncation for this byte store.
            ((f64::from(noise) + 1.) * 127.5) as u8
        });
        self.phase = (phase + delta * 1.5) as f32;
        if self.fade < 0.75 {
            self.fade = (self.fade + delta_seconds).min(0.75);
        }
        WorldNetherFrame {
            colors,
            angle,
            fade: self.fade,
        }
    }
}

fn noise(random: &mut M2ParticleRandom) -> f32 {
    (2. * random.next_unit() - 1.) % 1.
}

#[cfg(test)]
#[path = "../../../tests/unit/nether_screen.rs"]
mod tests;
