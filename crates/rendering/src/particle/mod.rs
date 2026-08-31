//! Particle simulation inputs, GPU buffers, and particle draw submission.
//!
//! The stock `ParticleSystem2.cpp` family establishes this specialized path.
//! ECS effects and spell systems provide state without owning GPU resources.

mod particle_system2;
mod ribbon;

pub use ribbon::{
    M2RibbonControlPoint, M2RibbonPose, M2RibbonSection, M2RibbonTrail, M2RibbonTrailError,
};
