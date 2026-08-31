//! Particle simulation inputs, GPU buffers, and particle draw submission.
//!
//! The stock `ParticleSystem2.cpp` family establishes this specialized path.
//! ECS effects and spell systems provide state without owning GPU resources.

mod particle_system2;
mod ribbon;

pub use particle_system2::{
    M2ParticleLifetimePose, M2ParticleLifetimePoseError, M2ParticleMeshPlan,
    M2ParticleMeshPlanError, M2ParticlePose, M2ParticleRandom, M2ParticleRenderVertex,
    M2ParticleRotationPose, M2ParticleSimulation, M2ParticleSimulationError,
    M2ParticleSimulationReport, M2ParticleState, M2ParticleStateError, M2ParticleTwinkleError,
    M2ParticleTwinkleTable,
};

/// Reproduces stock's float-to-D3DCOLOR conversion and memory byte order.
fn pack_bgra(color: [f32; 4]) -> [u8; 4] {
    let [red, green, blue, alpha] = color.map(quantize_color);
    [blue, green, red, alpha]
}

fn quantize_color(component: f32) -> u8 {
    (component.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
pub use ribbon::{
    M2RibbonControlPoint, M2RibbonMeshPlan, M2RibbonMeshPlanError, M2RibbonPose,
    M2RibbonRenderVertex, M2RibbonSection, M2RibbonTrail, M2RibbonTrailError,
};
