//! Stable failures from particle shader compilation.

use thiserror::Error;

/// Failure to initialize or compile the ordinary particle shader pair.
#[derive(Debug, Error)]
pub enum M2ParticleSpirvError {
    /// The pinned shader compiler could not be created.
    #[error("M2 particle SPIR-V compiler initialization failed: {message}")]
    Initialization {
        /// Compiler diagnostic.
        message: String,
    },
    /// GLSL-to-SPIR-V compilation rejected one translated stage.
    #[error("M2 particle {stage} shader compilation failed: {message}")]
    Compilation {
        /// Human-readable shader stage.
        stage: &'static str,
        /// Compiler diagnostic.
        message: String,
    },
}
