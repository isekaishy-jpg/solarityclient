//! Stable failures from ribbon shader compilation.

use thiserror::Error;

/// Failure to initialize or compile the exact ribbon shader pair.
#[derive(Debug, Error)]
pub enum M2RibbonSpirvError {
    /// The pinned shader compiler could not be created.
    #[error("M2 ribbon SPIR-V compiler initialization failed: {message}")]
    Initialization {
        /// Compiler diagnostic.
        message: String,
    },
    /// GLSL-to-SPIR-V compilation rejected one translated stage.
    #[error("M2 ribbon {stage} shader compilation failed: {message}")]
    Compilation {
        /// Human-readable shader stage.
        stage: &'static str,
        /// Compiler diagnostic.
        message: String,
    },
}
