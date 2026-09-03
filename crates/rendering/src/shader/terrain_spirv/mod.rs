//! Stock terrain layer composition compiled for Vulkan 1.3 and SPIR-V 1.6.

mod compiler;
mod status;
mod types;

pub use compiler::TerrainSpirvCompiler;
pub use status::{TerrainLayerCountError, TerrainSpirvError};
pub use types::{TerrainLayerCount, TerrainSpirvProgram};
