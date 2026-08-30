//! Stable failures from the UI shader compilation boundary.

use thiserror::Error;

/// Failure to initialize or compile one exact simple-render shader pair.
#[derive(Debug, Error)]
pub enum UiSpirvError {
    /// The pinned shader compiler could not be created.
    #[error("UI SPIR-V compiler initialization failed: {message}")]
    Initialization {
        /// Compiler diagnostic.
        message: String,
    },
    /// GLSL-to-SPIR-V compilation rejected one translated stage.
    #[error("UI {stage} shader compilation failed: {message}")]
    Compilation {
        /// Human-readable shader stage.
        stage: &'static str,
        /// Compiler diagnostic.
        message: String,
    },
}
