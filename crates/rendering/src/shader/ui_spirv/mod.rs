//! Stock UI texture/color behavior compiled for Vulkan 1.3 and SPIR-V 1.6.

mod compiler;
mod source;
mod status;
mod types;

pub use compiler::UiSpirvCompiler;
pub use status::UiSpirvError;
pub use types::{UiShaderSource, UiSpirvProgram};
