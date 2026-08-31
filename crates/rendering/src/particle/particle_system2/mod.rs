//! Stock `ParticleSystem2` animation inputs and placement-local state.
//!
//! Build 12340 constructs one mutable system per placed emitter. Immutable M2
//! animation sampling is kept separate from the emitter-owned random stream and
//! live-particle storage that subsequent modules add.

mod lifetime;
mod pose;
mod random;
mod state;

pub use lifetime::{M2ParticleLifetimePose, M2ParticleLifetimePoseError};
pub use pose::M2ParticlePose;
pub use random::M2ParticleRandom;
pub use state::{M2ParticleState, M2ParticleStateError};
