//! Stable failures while translating live UI state for the renderer.

use solarity_rendering::UiMeshPlanError;
use thiserror::Error;

/// Post-Lua presentation state could not enter the renderer-owned mesh ABI.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum UiRenderError {
    /// Logical positions, colors, or mesh counts violate the GPU boundary.
    #[error(transparent)]
    Mesh(#[from] UiMeshPlanError),
}
