//! Stock `ParticleSystem2` animation inputs and placement-local state.
//!
//! Build 12340 constructs one mutable system per placed emitter. Immutable M2
//! animation sampling is kept separate from the emitter-owned random stream and
//! live-particle storage that subsequent modules add.

mod lifetime;
mod mesh;
mod particle_color;
mod pose;
mod random;
mod rotation;
mod simulation;
mod state;
mod twinkle;

pub use lifetime::{M2ParticleLifetimePose, M2ParticleLifetimePoseError};
pub use mesh::{M2ParticleMeshPlan, M2ParticleMeshPlanError, M2ParticleRenderVertex};
pub use particle_color::M2ParticleColorReplacement;
pub use pose::M2ParticlePose;
pub use random::M2ParticleRandom;
pub use rotation::M2ParticleRotationPose;
pub use simulation::{M2ParticleSimulation, M2ParticleSimulationError, M2ParticleSimulationReport};
pub use state::{M2ParticleState, M2ParticleStateError};
pub use twinkle::{M2ParticleTwinkleError, M2ParticleTwinkleTable};
