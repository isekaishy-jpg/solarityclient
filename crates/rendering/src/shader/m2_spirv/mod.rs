//! Stock M2 behavior translated to cacheable Vulkan 1.3 shader modules.

mod compiler;
mod source;
mod status;
mod types;

pub use compiler::M2SpirvCompiler;
pub use status::M2SpirvError;
pub use types::{M2SpirvKey, M2SpirvProgram};
