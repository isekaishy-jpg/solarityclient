//! Bounded placement-local emission and lifecycle ownership.

use glam::{Mat4, Vec3};
use solarity_asset::M2ParticleEmitter;
use thiserror::Error;

use super::{M2ParticlePose, M2ParticleRandom, M2ParticleState, M2ParticleStateError};

/// Executable constant at `0x009F23CC` used to overprovision emitter storage.
const STOCK_CAPACITY_HEADROOM: f64 = f32::from_bits(0x3F93_3333) as f64;

/// `CM2ParticleEmitter::Update` consumes at most 100 ms per simulation step.
const STOCK_MAXIMUM_STEP_SECONDS: f32 = 0.1;

/// Particles stay in emitter-local space and receive the live bone matrix.
const PARTICLES_IN_MODEL_SPACE: u32 = 0x0000_0010;

/// Adds sampled emitter motion to newly born particle velocity.
const INHERIT_VELOCITY: u32 = 0x0000_0040;

/// Moves established particles by a speed-scaled share of emitter motion.
const FOLLOW_POSITION: u32 = 0x0000_4000;

/// Build 12340 normalizes inherited emitter displacement to a 30 ms sample.
const EMITTER_MOTION_SAMPLE_SECONDS: f32 = 0.03;

/// Sphere particles launch along local +Z rather than away from the center.
const SPHERE_VERTICAL_VELOCITY: u32 = 0x0000_8000;

/// Recovered behaviors which must not be silently replaced by basic motion.
const UNSUPPORTED_SIMULATION_FLAGS: u32 = 0x0000_0800 | 0x0000_1000 | 0x0000_2000 | 0x0080_0000;

/// Placement-local stock particle storage, emission remainder, and PRNG.
#[derive(Clone, Debug)]
pub struct M2ParticleSimulation {
    particles: Vec<M2ParticleState>,
    capacity: usize,
    emission_remainder: f32,
    seed: u32,
    random: M2ParticleRandom,
    previous_follow_position: Option<Vec3>,
    inherited_motion: InheritedEmitterMotion,
}

/// Per-emitter history for build 12340's 30 ms inherited-motion sampler.
#[derive(Clone, Copy, Debug, Default)]
struct InheritedEmitterMotion {
    previous_position: Option<Vec3>,
    sampled_motion: Vec3,
    accumulated_seconds: f32,
}

impl InheritedEmitterMotion {
    fn sample(
        &mut self,
        current_position: Vec3,
        elapsed_seconds: f32,
        scale: f32,
        enabled: bool,
        has_live_particles: bool,
    ) -> Vec3 {
        let Some(previous_position) = self.previous_position else {
            self.previous_position = Some(current_position);
            return Vec3::ZERO;
        };
        if !enabled {
            self.previous_position = Some(current_position);
            self.sampled_motion = Vec3::ZERO;
            self.accumulated_seconds = 0.0;
            return Vec3::ZERO;
        }
        if elapsed_seconds > 0.0 {
            self.accumulated_seconds += elapsed_seconds;
            if self.accumulated_seconds >= EMITTER_MOTION_SAMPLE_SECONDS {
                self.sampled_motion = if has_live_particles {
                    Vec3::ZERO
                } else {
                    (current_position - previous_position)
                        * (EMITTER_MOTION_SAMPLE_SECONDS / self.accumulated_seconds * scale)
                };
                self.previous_position = Some(current_position);
                self.accumulated_seconds = 0.0;
            }
        }
        self.sampled_motion
    }
}

impl M2ParticleSimulation {
    /// Creates an emitter owner from its runtime-provided stock seed.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self {
            particles: Vec::new(),
            capacity: 0,
            emission_remainder: 0.0,
            seed,
            random: M2ParticleRandom::new(seed),
            previous_follow_position: None,
            inherited_motion: InheritedEmitterMotion::default(),
        }
    }

    /// Clears one presentation-discontinuous emitter back to its construction
    /// seed without changing placement ownership.
    pub fn reset(&mut self) {
        self.particles.clear();
        self.capacity = 0;
        self.emission_remainder = 0.0;
        self.random = M2ParticleRandom::new(self.seed);
        self.previous_follow_position = None;
        self.inherited_motion = InheritedEmitterMotion::default();
    }

    /// Emits planar particles, advances all live particles, and removes deaths.
    ///
    /// This covers stock emitter type `1`, including inherited velocity and
    /// follow-position motion. Spline and collision paths return typed errors
    /// until their recovered implementations are selected.
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
        self.advance_with_emitter_motion(
            emitter,
            pose,
            elapsed_seconds,
            emitter_transform,
            density,
            EmitterShape::Plane,
        )
    }

    /// Advances a planar emitter through build-12340's bounded update slices.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::advance_planar`].
    pub fn advance_planar_bounded(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        elapsed_seconds: f32,
        emitter_transform: Mat4,
        density: f32,
    ) -> Result<M2ParticleSimulationReport, M2ParticleSimulationError> {
        self.advance_bounded(
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
        self.advance_with_emitter_motion(
            emitter,
            pose,
            elapsed_seconds,
            emitter_transform,
            density,
            EmitterShape::Sphere,
        )
    }

    /// Advances a spherical emitter through build-12340's bounded slices.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::advance_sphere`].
    pub fn advance_sphere_bounded(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        elapsed_seconds: f32,
        emitter_transform: Mat4,
        density: f32,
    ) -> Result<M2ParticleSimulationReport, M2ParticleSimulationError> {
        self.advance_bounded(
            emitter,
            pose,
            elapsed_seconds,
            emitter_transform,
            density,
            EmitterShape::Sphere,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn advance_bounded(
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
        if elapsed_seconds == 0.0 {
            return self.advance_with_emitter_motion(
                emitter,
                pose,
                0.0,
                emitter_transform,
                density,
                shape,
            );
        }
        validate_update_inputs(elapsed_seconds, emitter_transform, density)?;
        let (follow_delta, inherited_velocity) =
            self.emitter_motion(emitter, elapsed_seconds, emitter_transform);
        let mut remaining = elapsed_seconds;
        let mut first_step = true;
        let mut report = M2ParticleSimulationReport {
            emitted: 0,
            deaths: 0,
            live: self.particles.len(),
        };
        while remaining > 0.0 {
            let step = remaining.min(STOCK_MAXIMUM_STEP_SECONDS);
            let current = self.advance(
                emitter,
                pose,
                step,
                emitter_transform,
                density,
                shape,
                if first_step { follow_delta } else { Vec3::ZERO },
                inherited_velocity,
            )?;
            report.emitted += current.emitted;
            report.deaths += current.deaths;
            report.live = current.live;
            remaining = (remaining - step).max(0.0);
            first_step = false;
        }
        Ok(report)
    }

    /// Samples emitter history once before one unbounded simulation update.
    fn advance_with_emitter_motion(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        elapsed_seconds: f32,
        emitter_transform: Mat4,
        density: f32,
        shape: EmitterShape,
    ) -> Result<M2ParticleSimulationReport, M2ParticleSimulationError> {
        validate_update_inputs(elapsed_seconds, emitter_transform, density)?;
        let (follow_delta, inherited_velocity) =
            self.emitter_motion(emitter, elapsed_seconds, emitter_transform);
        self.advance(
            emitter,
            pose,
            elapsed_seconds,
            emitter_transform,
            density,
            shape,
            follow_delta,
            inherited_velocity,
        )
    }

    /// Resolves follow-position and inherited-velocity state from one live pose.
    fn emitter_motion(
        &mut self,
        emitter: &M2ParticleEmitter,
        elapsed_seconds: f32,
        emitter_transform: Mat4,
    ) -> (Vec3, Vec3) {
        let current_position = emitter_transform.transform_point3(Vec3::ZERO);
        let follow_delta = self
            .previous_follow_position
            .map_or(Vec3::ZERO, |previous| {
                if emitter.flags() & FOLLOW_POSITION == 0 {
                    return Vec3::ZERO;
                }
                let delta = current_position - previous;
                let (speed_minimum, speed_maximum) = emitter.follow_speed();
                let (scale_minimum, scale_maximum) = emitter.follow_scale();
                let denominator = speed_maximum - speed_minimum;
                let (multiplier, base) = if denominator.abs() > 0.000_001 {
                    let multiplier = (scale_maximum - scale_minimum) / denominator;
                    (multiplier, scale_minimum - speed_minimum * multiplier)
                } else {
                    (0.0, 0.0)
                };
                let speed = delta.length() / elapsed_seconds.max(0.000_001);
                delta * (multiplier * speed + base).clamp(0.0, 1.0)
            });
        self.previous_follow_position = Some(current_position);
        let inherited_velocity = self.inherited_motion.sample(
            current_position,
            elapsed_seconds,
            emitter.inherit_velocity_scale(),
            emitter.flags() & INHERIT_VELOCITY != 0,
            !self.particles.is_empty(),
        );
        (follow_delta, inherited_velocity)
    }

    /// Runs emission and live-particle advancement shared by stock shapes.
    #[allow(clippy::too_many_arguments)]
    fn advance(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
        elapsed_seconds: f32,
        emitter_transform: Mat4,
        density: f32,
        shape: EmitterShape,
        follow_delta: Vec3,
        inherited_velocity: Vec3,
    ) -> Result<M2ParticleSimulationReport, M2ParticleSimulationError> {
        validate_update_inputs(elapsed_seconds, emitter_transform, density)?;
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
        self.grow_stock_capacity(emitter, pose)?;

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
            // Build-12340 CParticleEmitter::Update adds one half and truncates
            // the accumulated count. This can leave a negative residual after
            // a birth and deliberately centers the first low-rate birth.
            let requested = (self.emission_remainder + 0.5).floor().max(0.0) as usize;
            let admitted = requested.min(self.capacity.saturating_sub(self.particles.len()));
            for _ in 0..admitted {
                let particle = match shape {
                    EmitterShape::Plane => spawn_planar(
                        emitter,
                        pose,
                        elapsed_seconds,
                        emitter_transform,
                        &mut self.random,
                        inherited_velocity,
                    )?,
                    EmitterShape::Sphere => spawn_sphere(
                        emitter,
                        pose,
                        elapsed_seconds,
                        emitter_transform,
                        &mut self.random,
                        inherited_velocity,
                    )?,
                };
                self.particles.push(particle);
                emitted += 1;
            }
            self.emission_remainder -= emitted as f32;
            // Stock discards a backlog larger than two particles when the
            // fixed-capacity pool cannot admit the requested births.
            if self.emission_remainder > 2.0 {
                self.emission_remainder = 0.0;
            }
        }

        if follow_delta != Vec3::ZERO {
            let displacement = if emitter.flags() & PARTICLES_IN_MODEL_SPACE != 0 {
                emitter_transform.inverse().transform_vector3(follow_delta)
            } else {
                follow_delta
            };
            let existing_count = self.particles.len().saturating_sub(emitted);
            for particle in &mut self.particles[..existing_count] {
                if particle.age_seconds() + elapsed_seconds > 2.0 * elapsed_seconds {
                    particle.translate(displacement);
                }
            }
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

    /// Returns the current grow-only stock particle-pool limit.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the fractional emission count carried into the next update.
    #[must_use]
    pub const fn emission_remainder(&self) -> f32 {
        self.emission_remainder
    }

    /// Grows the particle pool using build 12340's current-rate estimate.
    ///
    /// The original evaluates the float inputs in x87 extended precision,
    /// rounds to nearest-even, and only reallocates when the estimate grows.
    fn grow_stock_capacity(
        &mut self,
        emitter: &M2ParticleEmitter,
        pose: M2ParticlePose,
    ) -> Result<(), M2ParticleSimulationError> {
        let rate = f64::from(pose.emission_rate()) + f64::from(emitter.emission_rate_variation());
        let lifetime = f64::from(pose.lifespan()) + f64::from(emitter.lifespan_variation());
        let estimate = rate * lifetime * STOCK_CAPACITY_HEADROOM;
        if !estimate.is_finite() || estimate < 0.0 || estimate > usize::MAX as f64 {
            return Err(M2ParticleSimulationError::Capacity);
        }
        let required = estimate.round_ties_even() as usize;
        if required > self.capacity {
            self.particles
                .try_reserve_exact(required.saturating_sub(self.particles.len()))
                .map_err(|_| M2ParticleSimulationError::Allocation)?;
            self.capacity = required;
        }
        Ok(())
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
    inherited_motion: Vec3,
) -> Result<M2ParticleState, M2ParticleSimulationError> {
    let age = random.next_unit() * elapsed_seconds;
    let random_word = random.next_u32() as u16;
    // CPlaneParticleEmitter::CreateParticle evaluates the Y coordinate before
    // X, then consumes speed and the two spread angles in that order.
    let position_y = random.next_signed() * pose.emission_area_width() * 0.5;
    let position_x = random.next_signed() * pose.emission_area_length() * 0.5;
    let mut position = Vec3::new(position_x, position_y, 0.0);
    let speed = (random.next_signed() * pose.speed_variation() + 1.0) * pose.emission_speed();
    // Both spread samples are consumed even when z-source replaces the
    // resulting direction. Later particles therefore retain stock PRNG phase.
    let polar = random.next_signed() * pose.vertical_range();
    let azimuth = random.next_signed() * pose.horizontal_range();
    let mut velocity = if pose.z_source() == 0.0 {
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
    let inherited_velocity = varied_inherited_velocity(emitter, pose, random, inherited_motion);
    if emitter.flags() & PARTICLES_IN_MODEL_SPACE == 0 {
        velocity = emitter_transform.transform_vector3(velocity);
        position = emitter_transform.transform_point3(position);
        velocity += inherited_velocity;
    } else {
        velocity += emitter_transform
            .inverse()
            .transform_vector3(inherited_velocity);
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
    inherited_motion: Vec3,
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
    let inherited_velocity = varied_inherited_velocity(emitter, pose, random, inherited_motion);
    if emitter.flags() & PARTICLES_IN_MODEL_SPACE == 0 {
        direction = emitter_transform.transform_vector3(direction);
        position = emitter_transform.transform_point3(position);
        direction += inherited_velocity;
    } else {
        direction += emitter_transform
            .inverse()
            .transform_vector3(inherited_velocity);
    }
    M2ParticleState::new(age, position, direction, random_word).map_err(Into::into)
}

/// Applies the second speed-variation sample used by build 12340 only when
/// inherited motion is selected. The conditional PRNG consumption is part of
/// the executable's per-particle call order (`0x00981950`).
fn varied_inherited_velocity(
    emitter: &M2ParticleEmitter,
    pose: M2ParticlePose,
    random: &mut M2ParticleRandom,
    inherited_motion: Vec3,
) -> Vec3 {
    if emitter.flags() & INHERIT_VELOCITY == 0 {
        return Vec3::ZERO;
    }
    inherited_motion * (random.next_signed() * pose.speed_variation() + 1.0)
}

fn validate_update_inputs(
    elapsed_seconds: f32,
    emitter_transform: Mat4,
    density: f32,
) -> Result<(), M2ParticleSimulationError> {
    if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
        return Err(M2ParticleSimulationError::ElapsedTime);
    }
    if !density.is_finite() || density < 0.0 {
        return Err(M2ParticleSimulationError::Density);
    }
    if !emitter_transform.is_finite() {
        return Err(M2ParticleSimulationError::Transform);
    }
    Ok(())
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
    /// Current authored rate and lifetime cannot produce a stock pool size.
    #[error("particle capacity estimate is outside the native allocation domain")]
    Capacity,
    /// The native allocator could not grow the stock particle pool.
    #[error("particle pool allocation failed")]
    Allocation,
    /// A z-source emitter cannot aim from a coincident source point.
    #[error("particle z-source aim is degenerate")]
    DegenerateAim,
    /// Spawned or advanced common state was invalid.
    #[error(transparent)]
    State(#[from] M2ParticleStateError),
}
