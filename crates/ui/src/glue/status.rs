//! Stable GlueXML startup failures.

use thiserror::Error;

use crate::{
    FontError, UiFrameError, UiLayoutError, UiLoadError, UiObjectError, UiScriptError,
    UiTextureError,
};

/// A stock built-in login UI could not be constructed or executed.
#[derive(Debug, Error)]
pub enum GlueError {
    /// Manifest, archive resource, XML, or Lua compilation failed.
    #[error(transparent)]
    Load(#[from] UiLoadError),
    /// A global font definition is malformed.
    #[error(transparent)]
    Font(#[from] FontError),
    /// Template or live object construction failed.
    #[error(transparent)]
    Object(#[from] UiObjectError),
    /// Frame property decoding or parent resolution failed.
    #[error(transparent)]
    Frame(#[from] UiFrameError),
    /// Region geometry or anchor resolution failed.
    #[error(transparent)]
    Layout(#[from] UiLayoutError),
    /// Texture declaration decoding failed.
    #[error(transparent)]
    Texture(#[from] UiTextureError),
    /// Lua plan creation or ordered execution failed.
    #[error(transparent)]
    Script(#[from] UiScriptError),
}
