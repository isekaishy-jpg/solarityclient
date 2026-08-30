//! Stable failures from WMO effect selection and shader compilation.

use thiserror::Error;

/// Failure to select or compile one exact stock MapObj effect pair.
#[derive(Debug, Error)]
pub enum WorldModelSpirvError {
    /// The ordinary effect table deliberately contains no selector-six entry.
    #[error("ordinary MapObj shader 6 selects stock's null effect slot")]
    OrdinaryComposite,
    /// The pinned shader compiler could not be created.
    #[error("WMO SPIR-V compiler initialization failed: {message}")]
    Initialization {
        /// Compiler diagnostic.
        message: String,
    },
    /// GLSL-to-SPIR-V compilation rejected one translated stage.
    #[error("WMO {stage} shader compilation failed: {message}")]
    Compilation {
        /// Human-readable shader stage.
        stage: &'static str,
        /// Compiler diagnostic.
        message: String,
    },
}
