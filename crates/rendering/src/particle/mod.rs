//! Particle simulation inputs, GPU buffers, and particle draw submission.
//!
//! The stock `ParticleSystem2.cpp` family establishes this specialized path.
//! ECS effects and spell systems provide state without owning GPU resources.

mod particle_system2;
mod ribbon;

pub use particle_system2::{
    M2ParticleLifetimePose, M2ParticleLifetimePoseError, M2ParticlePose, M2ParticleRandom,
    M2ParticleRotationPose, M2ParticleSimulation, M2ParticleSimulationError,
    M2ParticleSimulationReport, M2ParticleState, M2ParticleStateError,
};
pub use ribbon::{
    M2RibbonControlPoint, M2RibbonMeshPlan, M2RibbonMeshPlanError, M2RibbonPose,
    M2RibbonRenderVertex, M2RibbonSection, M2RibbonTrail, M2RibbonTrailError,
};
