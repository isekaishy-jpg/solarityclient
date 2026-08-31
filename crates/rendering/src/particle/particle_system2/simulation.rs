//! Bounded placement-local emission and lifecycle ownership.

use glam::{Mat4, Vec3};
use solarity_asset::M2ParticleEmitter;
use thiserror::Error;

use super::{M2ParticlePose, M2ParticleRandom, M2ParticleState, M2ParticleStateError};

/// Particles stay in emitter-local space instead of receiving the bone matrix.
const PARTICLES_IN_MODEL_SPACE: u32 = 0x0000_0200;

/// Sphere particles launch along local +Z rather than away from the center.
const SPHERE_VERTICAL_VELOCITY: u32 = 0x0000_8000;

/// Recovered behaviors which must not be silently replaced by basic motion.
const UNSUPPORTED_SIMULATION_FLAGS: u32 =
    0x0000_0800 | 0x0000_1000 | 0x0000_2000 | 0x0004_0000 | 0x0008_0000;

/// Placement-local stock particle storage, emission remainder, and PRNG.
#[derive(Clone, Debug)]
pub struct M2ParticleSimulation {
    particles: Vec<M2ParticleState>,
    capacity: usize,
    emission_remainder: f32,
    random: M2ParticleRandom,
}

impl M2ParticleSimulation {
    /// Creates a bounded emitter owner from its runtime-provided stock seed.
    #[must_use]
    pub fn new(seed: u32, capacity: usize) -> Self {
        Self {
            particles: Vec::with_capacity(capacity),
            capacity,
            emission_remainder: 0.0,
            random: M2ParticleRandom::new(seed),
        }
    }

    /// Emits planar particles, advances all live particles, and removes deaths.
    ///
    /// This covers stock emitter type `1`. Spline, collision, inherited
    /// velocity, and follow-position paths use separate entry points or return
    /// typed errors until their recovered implementations are selected.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleSimulationError`] for invalid update inputs, an
    /// unsupported stock behavior, or invalid state produced by authored data.
    pub fn advance_planar(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        elapsed_seconds: f32,
        emitter_transform: Mat4,
        density: f32,
    ) -> Result<M2ParticleSimulationReport, M2ParticleSimulationError> {
        self.advance(
            emitter,
            pose,
            elapsed_seconds,
            emitter_transform,
            density,
            EmitterShape::Plane,
        )
    }

    /// Emits spherical-shell particles and advances the shared lifecycle.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleSimulationError`] under the same conditions as
    /// [`Self::advance_planar`], or when the emitter is not stock type `2`.
    pub fn advance_sphere(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        elapsed_seconds: f32,
        emitter_transform: Mat4,
        density: f32,
    ) -> Result<M2ParticleSimulationReport, M2ParticleSimulationError> {
        self.advance(
            emitter,
            pose,
            elapsed_seconds,
            emitter_transform,
            density,
            EmitterShape::Sphere,
        )
    }

    /// Runs emission and live-particle advancement shared by stock shapes.
    fn advance(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        elapsed_seconds: f32,
        emitter_transform: Mat4,
        density: f32,
        shape: EmitterShape,
    ) -> Result<M2ParticleSimulationReport, M2ParticleSimulationError> {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return Err(M2ParticleSimulationError::ElapsedTime);
        }
        if !density.is_finite() || density < 0.0 {
            return Err(M2ParticleSimulationError::Density);
        }
        if !emitter_transform.is_finite() {
            return Err(M2ParticleSimulationError::Transform);
        }
        if emitter.emitter_type() != shape.selector() {
            return Err(M2ParticleSimulationError::EmitterType {
                expected: shape.selector(),
                actual: emitter.emitter_type(),
            });
        }
        let unsupported = emitter.flags() & UNSUPPORTED_SIMULATION_FLAGS;
        if unsupported != 0 {
            return Err(M2ParticleSimulationError::BehaviorFlags(unsupported));
        }

        // The executable samples rate variation once per update before testing
        // whether emission is enabled, preserving PRNG call order across keys.
        let varied_rate = (pose.emission_rate()
            + self.random.next_signed() * emitter.emission_rate_variation())
            * density;
        if !varied_rate.is_finite() || varied_rate < 0.0 {
            return Err(M2ParticleSimulationError::EmissionRate);
        }
        let mut emitted = 0;
        if pose.enabled() {
            self.emission_remainder += varied_rate * elapsed_seconds;
            let requested = (self.emission_remainder + 0.5).round_ties_even() as usize;
            let admitted = requested.min(self.capacity.saturating_sub(self.particles.len()));
            for _ in 0..admitted {
                let particle = match shape {
                    EmitterShape::Plane => spawn_planar(
                        emitter,
                        pose,
                        elapsed_seconds,
                        emitter_transform,
                        &mut self.random,
                    )?,
                    EmitterShape::Sphere => spawn_sphere(
                        emitter,
                        pose,
                        elapsed_seconds,
                        emitter_transform,
                        &mut self.random,
                    )?,
                };
                self.particles.push(particle);
                emitted += 1;
            }
            self.emission_remainder -= emitted as f32;
        }

        let mut deaths = 0;
        let mut index = 0;
        while index < self.particles.len() {
            let particle = &mut self.particles[index];
            if particle.age_seconds() < emitter.wind_time() {
                particle.add_velocity(emitter.wind_vector() * elapsed_seconds);
            }
            particle.advance(elapsed_seconds, pose.gravity(), emitter.drag())?;
            if particle.is_alive(pose.lifespan(), emitter.lifespan_variation()) {
                index += 1;
            } else {
                self.particles.swap_remove(index);
                deaths += 1;
            }
        }
        Ok(M2ParticleSimulationReport {
            emitted,
            deaths,
            live: self.particles.len(),
        })
    }

    /// Returns live particles in the simulation's unstable stock storage order.
    #[must_use]
    pub fn particles(&self) -> &[M2ParticleState] {
        &self.particles
    }

    /// Returns the preallocated maximum number of live particles.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the fractional emission count carried into the next update.
    #[must_use]
    pub const fn emission_remainder(&self) -> f32 {
        self.emission_remainder
    }
}

/// Generator selector retained separately from raw authored bytes.
#[derive(Clone, Copy)]
enum EmitterShape {
    Plane,
    Sphere,
}

impl EmitterShape {
    const fn selector(self) -> u8 {
        match self {
            Self::Plane => 1,
            Self::Sphere => 2,
        }
    }
}

/// Emits one ordinary planar particle in executable call order.
fn spawn_planar(
    emitter: &M2ParticleEmitter,
    pose: M2ParticlePose,
    elapsed_seconds: f32,
    emitter_transform: Mat4,
    random: &mut M2ParticleRandom,
) -> Result<M2ParticleState, M2ParticleSimulationError> {
    let age = random.next_unit() * elapsed_seconds;
    let random_word = random.next_u32() as u16;
    let mut position = Vec3::new(
        random.next_signed() * pose.emission_area_length() * 0.5,
        random.next_signed() * pose.emission_area_width() * 0.5,
        0.0,
    );
    let speed = (random.next_signed() * pose.speed_variation() + 1.0) * pose.emission_speed();
    let mut velocity = if pose.z_source() == 0.0 {
        let polar = random.next_signed() * pose.vertical_range();
        let azimuth = random.next_signed() * pose.horizontal_range();
        Vec3::new(
            azimuth.cos() * polar.sin(),
            azimuth.sin() * polar.sin(),
            polar.cos(),
        ) * speed
    } else {
        let aim = position - Vec3::Z * pose.z_source();
        if aim.length_squared() == 0.0 {
            return Err(M2ParticleSimulationError::DegenerateAim);
        }
        aim.normalize() * speed
    };
    if emitter.flags() & PARTICLES_IN_MODEL_SPACE == 0 {
        velocity = emitter_transform.transform_vector3(velocity);
        position = emitter_transform.transform_point3(position);
    }
    M2ParticleState::new(age, position, velocity, random_word).map_err(Into::into)
}

/// Emits one spherical-shell particle in executable `0x00981950` call order.
fn spawn_sphere(
    emitter: &M2ParticleEmitter,
    pose: M2ParticlePose,
    elapsed_seconds: f32,
    emitter_transform: Mat4,
    random: &mut M2ParticleRandom,
) -> Result<M2ParticleState, M2ParticleSimulationError> {
    let age = random.next_unit() * elapsed_seconds;
    let random_word = random.next_u32() as u16;
    let minimum_radius = pose.emission_area_length();
    let radius =
        minimum_radius + random.next_unit() * (pose.emission_area_width() - minimum_radius);
    let elevation = random.next_signed() * pose.vertical_range();
    let azimuth = random.next_signed() * pose.horizontal_range();
    let mut position = Vec3::new(
        azimuth.cos() * elevation.cos(),
        azimuth.sin() * elevation.cos(),
        elevation.sin(),
    ) * radius;
    let mut direction = if pose.z_source() == 0.0 {
        if emitter.flags() & SPHERE_VERTICAL_VELOCITY != 0 {
            Vec3::Z
        } else {
            position.normalize_or_zero()
        }
    } else {
        let aim = position - Vec3::Z * pose.z_source();
        if aim.length_squared() == 0.0 {
            return Err(M2ParticleSimulationError::DegenerateAim);
        }
        aim.normalize()
    };
    let speed = (random.next_signed() * pose.speed_variation() + 1.0) * pose.emission_speed();
    direction *= speed;
    if emitter.flags() & PARTICLES_IN_MODEL_SPACE == 0 {
        direction = emitter_transform.transform_vector3(direction);
        position = emitter_transform.transform_point3(position);
    }
    M2ParticleState::new(age, position, direction, random_word).map_err(Into::into)
}

/// Counts produced by one complete emitter update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2ParticleSimulationReport {
    emitted: usize,
    deaths: usize,
    live: usize,
}

impl M2ParticleSimulationReport {
    /// Returns particles admitted during this update.
    #[must_use]
    pub const fn emitted(self) -> usize {
        self.emitted
    }

    /// Returns particles removed after reaching their varied lifetime.
    #[must_use]
    pub const fn deaths(self) -> usize {
        self.deaths
    }

    /// Returns particles remaining after the update.
    #[must_use]
    pub const fn live(self) -> usize {
        self.live
    }
}

/// An update cannot follow the recovered planar-particle path.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum M2ParticleSimulationError {
    /// Elapsed time must be finite and nonnegative.
    #[error("particle elapsed time must be finite and nonnegative")]
    ElapsedTime,
    /// The explicit stock density multiplier must be finite and nonnegative.
    #[error("particle density must be finite and nonnegative")]
    Density,
    /// The emitter transform contains a non-finite component.
    #[error("particle emitter transform must be finite")]
    Transform,
    /// This entry point only implements planar emitters.
    #[error("particle emitter type {actual} does not match required type {expected}")]
    EmitterType {
        /// Selector required by the chosen update entry point.
        expected: u8,
        /// Selector authored by the emitter.
        actual: u8,
    },
    /// One or more authored behavior bits require another recovered path.
    #[error("particle behavior flags 0x{0:08X} are not implemented")]
    BehaviorFlags(u32),
    /// Authored rate and variation produced an invalid effective rate.
    #[error("particle emission rate must be finite and nonnegative")]
    EmissionRate,
    /// A z-source emitter cannot aim from a coincident source point.
    #[error("particle z-source aim is degenerate")]
    DegenerateAim,
    /// Spawned or advanced common state was invalid.
    #[error(transparent)]
    State(#[from] M2ParticleStateError),
}
