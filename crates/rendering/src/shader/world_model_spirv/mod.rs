//! Stock MapObj effects translated to the pinned Vulkan/SPIR-V target.

mod compiler;
mod source;
mod status;
mod types;

pub use compiler::WorldModelSpirvCompiler;
pub use status::WorldModelSpirvError;
pub use types::{WorldModelSpirvKey, WorldModelSpirvProgram};
