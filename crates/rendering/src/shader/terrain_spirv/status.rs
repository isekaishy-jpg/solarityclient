//! Stable failures from the terrain shader boundary.

use thiserror::Error;

/// An authored terrain layer count lies outside stock's closed range.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("terrain chunks require one through four texture layers; found {layer_count}")]
pub struct TerrainLayerCountError {
    pub(super) layer_count: usize,
}

/// Failure to initialize or compile one terrain shader pair.
#[derive(Debug, Error)]
pub enum TerrainSpirvError {
    /// The pinned shader compiler could not be created.
    #[error("terrain SPIR-V compiler initialization failed: {message}")]
    Initialization {
        /// Compiler diagnostic.
        message: String,
    },
    /// GLSL-to-SPIR-V compilation rejected one translated stage.
    #[error("terrain {stage} shader compilation failed: {message}")]
    Compilation {
        /// Human-readable shader stage.
        stage: &'static str,
        /// Compiler diagnostic.
        message: String,
    },
}
