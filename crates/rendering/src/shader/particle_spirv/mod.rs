//! Stock particle texture/color behavior compiled for Vulkan 1.3 and SPIR-V 1.6.

mod compiler;
mod source;
mod status;
mod types;

pub use compiler::M2ParticleSpirvCompiler;
pub use status::M2ParticleSpirvError;
pub use types::M2ParticleSpirvProgram;
