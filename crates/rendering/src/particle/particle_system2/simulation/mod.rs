//! Placement-local simulation with separately owned stock capacity rules.

mod capacity;
mod state;
mod storage;

pub use state::{M2ParticleSimulation, M2ParticleSimulationError, M2ParticleSimulationReport};
