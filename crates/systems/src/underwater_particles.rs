//! Camera-relative particulate pool from build-12340's 79E100/79BF40.

use glam::Vec3;
use solarity_asset::LiquidTypeDefinition;
use thiserror::Error;

const PARTICLE_COUNT: usize = 4_000;
const CUBE_SIDE: f32 = 30.0;
const BASE_SIZE: f32 = 0.027_777_778;

/// Invalid camera position or elapsed time for the underwater particle pool.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("underwater particle camera/time is invalid")]
pub struct UnderwaterParticleError;

/// The original fixed-size pool, retaining particles while the camera is dry.
/// Random words come from the process's shared Blizzard stream.
pub struct UnderwaterParticles {
    particles: Vec<[f32; 4]>,
    previous_camera: Vec3,
    selected_liquid: u32,
    pool_liquid: u32,
    active: bool,
    base_size: f32,
    movement: u32,
    direction: Vec3,
    frequency: f32,
    phase: f32,
    speed: f32,
}

impl UnderwaterParticles {
    /// Initializes all 4,000 particles, then the drift direction, consuming
    /// 16,004 random words even before the first camera submersion.
    #[must_use]
    pub fn new(mut random: impl FnMut() -> u32) -> Self {
        let mut bank = Self {
            particles: vec![[0.0; 4]; PARTICLE_COUNT],
            previous_camera: Vec3::ZERO,
            selected_liquid: 0,
            pool_liquid: 2,
            active: false,
            base_size: BASE_SIZE,
            movement: 0,
            direction: Vec3::ZERO,
            frequency: 0.0,
            phase: 0.0,
            speed: 0.0,
        };
        bank.reseed(2, &mut random);
        bank.reset_drift(&mut random);
        bank
    }

    /// Applies 790920's new camera liquid after advancing the preceding frame.
    /// A new liquid reseeds using the previous base size before installing its
    /// authored scale. Dry cameras retain the pool and its last update origin.
    pub fn select_liquid(
        &mut self,
        liquid: Option<&LiquidTypeDefinition>,
        enabled: bool,
        mut random: impl FnMut() -> u32,
    ) {
        self.select(
            liquid.map_or(0, LiquidTypeDefinition::id),
            liquid.map_or(0, LiquidTypeDefinition::flags),
            liquid.map_or(0, LiquidTypeDefinition::particle_movement),
            liquid.map_or(1.0, LiquidTypeDefinition::particle_scale),
            enabled,
            &mut random,
        );
    }

    fn select(
        &mut self,
        id: u32,
        flags: u32,
        movement: u32,
        scale: f32,
        enabled: bool,
        random: &mut impl FnMut() -> u32,
    ) {
        if id != 0 && id != self.selected_liquid && enabled {
            self.active = flags & 8 != 0;
            if self.active {
                self.reseed(id, random);
                self.movement = movement;
                self.base_size = (f64::from(scale) * f64::from(BASE_SIZE)) as f32;
            }
        }
        self.selected_liquid = id;
    }

    /// Advances 79BF40 using the last selected liquid. Positions wrap once per
    /// axis; camera jumps longer than the cube side reseed the pool.
    ///
    /// # Errors
    /// Rejects non-finite positions or negative/non-finite elapsed seconds.
    pub fn advance(
        &mut self,
        camera: Vec3,
        seconds: f32,
        enabled: bool,
        mut random: impl FnMut() -> u32,
    ) -> Result<(), UnderwaterParticleError> {
        if !camera.is_finite() || !seconds.is_finite() || seconds < 0.0 {
            return Err(UnderwaterParticleError);
        }
        if !enabled || !self.visible() {
            return Ok(());
        }
        let mut delta = self.previous_camera - camera;
        self.previous_camera = camera;
        if delta.as_dvec3().length_squared() > f64::from(CUBE_SIDE).powi(2) {
            self.reseed(self.pool_liquid, &mut random);
            delta = Vec3::ZERO;
        }
        let drift = self.drift(seconds, &mut random);
        for particle in &mut self.particles {
            for axis in 0..3 {
                let mut value =
                    f64::from(particle[axis]) + f64::from(delta[axis]) + f64::from(drift[axis]);
                if value > f64::from(CUBE_SIDE) * 0.5 {
                    value -= f64::from(CUBE_SIDE);
                } else if value < -f64::from(CUBE_SIDE) * 0.5 {
                    value += f64::from(CUBE_SIDE);
                }
                particle[axis] = value as f32;
            }
        }
        Ok(())
    }

    /// Returns whether the current liquid admits this retained pool.
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.active && self.selected_liquid != 0
    }

    /// Returns the definition retained by the particle pool, which can differ
    /// from the camera liquid when the world render command disables updates.
    #[must_use]
    pub const fn pool_liquid_type(&self) -> u32 {
        self.pool_liquid
    }

    /// Borrows camera-relative XYZ and authored billboard size in native order.
    #[must_use]
    pub fn particles(&self) -> &[[f32; 4]] {
        &self.particles
    }

    fn reseed(&mut self, id: u32, random: &mut impl FnMut() -> u32) {
        let minimum = f64::from(self.base_size) * 0.5;
        let maximum = f64::from(self.base_size) * 1.5;
        for particle in &mut self.particles {
            let z = unit(random);
            let y = unit(random);
            let x = unit(random);
            *particle = [
                (x * f64::from(CUBE_SIDE) - 15.0) as f32,
                (y * f64::from(CUBE_SIDE) - 15.0) as f32,
                (z * f64::from(CUBE_SIDE) - 15.0) as f32,
                (unit(random) * (maximum - minimum) + minimum) as f32,
            ];
        }
        self.pool_liquid = id;
    }

    fn reset_drift(&mut self, random: &mut impl FnMut() -> u32) {
        let angle = (signed(random) * f64::from(std::f32::consts::PI)) as f32;
        let bearing = signed(random) * f64::from(std::f32::consts::PI);
        let sine = f64::from(angle).sin();
        self.direction = Vec3::new(
            (bearing.cos() * sine) as f32,
            (bearing.sin() * sine) as f32,
            (f64::from(angle).cos() * 0.25).abs() as f32,
        );
        self.direction = self.direction.as_dvec3().normalize().as_vec3();
        self.phase = 0.0;
        self.frequency = ((unit(random) + 1.0) * f64::from(0.0125_f32)) as f32;
        self.speed = ((unit(random) + 1.0) * f64::from(0.005_f32)) as f32;
    }

    fn drift(&mut self, seconds: f32, random: &mut impl FnMut() -> u32) -> Vec3 {
        match self.movement {
            0 => {
                let elapsed = f64::from(self.phase) + f64::from(seconds);
                self.phase = elapsed as f32;
                let mut phase = elapsed * f64::from(self.frequency);
                if phase > 0.5 {
                    self.reset_drift(random);
                    phase = 0.0;
                }
                let angle = (phase * f64::from(std::f32::consts::TAU)) as f32;
                let wave = particle_sine(angle) * f64::from(self.speed);
                (self.direction.as_dvec3() * wave).as_vec3()
            }
            1 => Vec3::new(0.0, 0.0, -0.02 * seconds),
            2 => Vec3::new(0.0, 0.0, 0.02 * seconds),
            _ => Vec3::ZERO,
        }
    }
}

/// 6F7A10 uses a periodic cubic approximation, not the CRT sine function.
fn particle_sine(angle: f32) -> f64 {
    let phase = (f64::from(angle) * f64::from(0.318_309_87_f32) - 0.5) as f32;
    let period = phase as i32 - i32::from(phase <= 0.0);
    let fraction = f64::from((f64::from(phase) - f64::from(period)) as f32);
    let value = 1.0 - (6.0 - 4.0 * fraction) * fraction * fraction;
    if period & 1 != 0 { -value } else { value }
}

fn unit(random: &mut impl FnMut() -> u32) -> f64 {
    f64::from(f32::from_bits((random() & 0x7f_ffff) | 0x3f80_0000)) - 1.0
}

fn signed(random: &mut impl FnMut() -> u32) -> f64 {
    let word = random();
    let value = f64::from(f32::from_bits((word & 0x7f_ffff) | 0x3f80_0000));
    if word & 0x8000_0000 != 0 {
        2.0 - value
    } else {
        value - 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Random {
        state: u32,
        draws: u32,
    }
    impl Random {
        fn next(&mut self) -> u32 {
            self.state = self
                .state
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            self.draws += 1;
            self.state
        }
    }

    fn snapshot(bank: &UnderwaterParticles, random: &Random) -> Vec<u32> {
        let mut result = vec![
            random.state,
            random.draws,
            bank.selected_liquid,
            bank.particles.len() as u32,
        ];
        result.extend(bank.previous_camera.to_array().map(f32::to_bits));
        result.extend([
            0,
            u32::from(bank.active),
            bank.base_size.to_bits(),
            CUBE_SIDE.to_bits(),
            0,
            bank.pool_liquid,
        ]);
        result.extend(bank.direction.to_array().map(f32::to_bits));
        result.extend([
            bank.frequency.to_bits(),
            bank.phase.to_bits(),
            bank.speed.to_bits(),
        ]);
        result
    }

    #[test]
    fn underwater_particle_constructors_match_original_complete_pools() {
        let fixture = include_bytes!("../tests/fixtures/underwater-particle-initial.bin");
        for record in fixture.as_chunks::<64080>().0 {
            let words: Vec<_> = record
                .as_chunks::<4>()
                .0
                .iter()
                .map(|word| u32::from_le_bytes(*word))
                .collect();
            let mut random = Random {
                state: words[0],
                draws: 0,
            };
            let bank = UnderwaterParticles::new(|| random.next());
            assert_eq!(snapshot(&bank, &random), words[1..20]);
            for (index, (particle, expected)) in bank
                .particles
                .iter()
                .zip(words[20..].as_chunks::<4>().0)
                .enumerate()
            {
                assert_eq!(
                    particle.map(f32::to_bits),
                    *expected,
                    "seed {} particle {index}",
                    words[0]
                );
            }
        }
    }

    #[test]
    fn underwater_particle_histories_match_original_liquid_edges_and_motion()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut current = None;
        let mut random = Random { state: 0, draws: 0 };
        let mut bank = UnderwaterParticles::new(|| 0);
        for row in include_str!("../tests/fixtures/underwater-particle-histories.txt")
            .lines()
            .filter(|row| !row.starts_with('#'))
        {
            let groups = row
                .split('|')
                .map(|group| {
                    group
                        .split_whitespace()
                        .map(|word| u32::from_str_radix(word, 16))
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            let key = (groups[0][0], groups[0][1]);
            if current != Some(key) {
                current = Some(key);
                random = Random {
                    state: key.0,
                    draws: 0,
                };
                bank = UnderwaterParticles::new(|| random.next());
                bank.particles.truncate(16);
            }
            let a = &groups[1];
            if a[0] == 1 {
                bank.select(
                    a[2],
                    a[3],
                    a[4],
                    f32::from_bits(a[5]),
                    a[1] != 0,
                    &mut || random.next(),
                );
            } else {
                bank.advance(
                    Vec3::new(
                        f32::from_bits(a[7]),
                        f32::from_bits(a[8]),
                        f32::from_bits(a[9]),
                    ),
                    f32::from_bits(a[6]),
                    a[1] != 0,
                    || random.next(),
                )?;
            }
            assert_eq!(snapshot(&bank, &random), groups[2], "{row}");
            for (index, (particle, expected)) in bank
                .particles
                .iter()
                .zip(groups[3].as_chunks::<4>().0)
                .enumerate()
            {
                assert_eq!(
                    particle.map(f32::to_bits),
                    *expected,
                    "particle {index}: {row}"
                );
            }
        }
        Ok(())
    }
}
