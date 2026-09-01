//! Stable GlueXML startup failures.

use thiserror::Error;

use solarity_asset::AssetError;

use crate::{
    FontError, UiAnimationError, UiFrameError, UiLayoutError, UiLoadError, UiObjectError,
    UiRenderError, UiScriptError, UiTextureError,
};

/// A stock built-in login UI could not be constructed or executed.
#[derive(Debug, Error)]
pub enum GlueError {
    /// Character-creation metadata could not be loaded from the client stack.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Manifest, archive resource, XML, or Lua compilation failed.
    #[error(transparent)]
    Load(#[from] UiLoadError),
    /// A global font definition is malformed.
    #[error(transparent)]
    Font(#[from] FontError),
    /// Template or live object construction failed.
    #[error(transparent)]
    Object(#[from] UiObjectError),
    /// Animation ownership or timeline properties are malformed.
    #[error(transparent)]
    Animation(#[from] UiAnimationError),
    /// Frame property decoding or parent resolution failed.
    #[error(transparent)]
    Frame(#[from] UiFrameError),
    /// Region geometry or anchor resolution failed.
    #[error(transparent)]
    Layout(#[from] UiLayoutError),
    /// Texture declaration decoding failed.
    #[error(transparent)]
    Texture(#[from] UiTextureError),
    /// Live presentation state cannot enter the renderer mesh ABI.
    #[error(transparent)]
    Render(#[from] UiRenderError),
    /// Lua plan creation or ordered execution failed.
    #[error(transparent)]
    Script(#[from] UiScriptError),
}
