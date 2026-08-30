//! Stable failures from the M2 shader translation boundary.

use thiserror::Error;

/// Failure to initialize or compile one exact stock M2 shader pair.
#[derive(Debug, Error)]
pub enum M2SpirvError {
    /// The pinned shader compiler could not be created.
    #[error("M2 SPIR-V compiler initialization failed: {message}")]
    Initialization {
        /// Compiler diagnostic.
        message: String,
    },
    /// A stock shadow permutation has not yet been translated faithfully.
    #[error("M2 shadow permutation is not translated: vertex {vertex_index}, pixel {pixel_index}")]
    UnsupportedShadow {
        /// Index into the selected vertex BLS.
        vertex_index: usize,
        /// Index into the selected pixel BLS.
        pixel_index: usize,
    },
    /// GLSL-to-SPIR-V compilation rejected one translated stage.
    #[error("M2 {stage} shader compilation failed: {message}")]
    Compilation {
        /// Human-readable shader stage.
        stage: &'static str,
        /// Compiler diagnostic.
        message: String,
    },
}
