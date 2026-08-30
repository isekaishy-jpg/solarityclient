//! Stable failures while translating live UI state for the renderer.

use solarity_asset::AssetError;
use solarity_rendering::UiMeshPlanError;
use thiserror::Error;

/// Post-Lua presentation state could not enter the renderer-owned mesh ABI.
#[derive(Debug, Error)]
pub enum UiRenderError {
    /// Logical positions, colors, or mesh counts violate the GPU boundary.
    #[error(transparent)]
    Mesh(#[from] UiMeshPlanError),
    /// A blocking stock BLP could not be read or parsed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Unique texture identities exceed the renderer's resource index.
    #[error("UI texture requests exceed unsigned 32-bit capacity")]
    TextureRequestCapacity,
}
