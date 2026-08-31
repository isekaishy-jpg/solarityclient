//! Stable failures at the stock key-binding declaration boundary.

use solarity_asset::{AssetError, AssetPath};
use thiserror::Error;

use crate::xml::UiLoadError;

/// A failure while loading or validating a `Bindings.xml` document.
#[derive(Debug, Error)]
pub enum UiBindingError {
    /// The selected archive or AddOn source could not be resolved.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Shared XML, text, or Lua validation rejected the document.
    #[error(transparent)]
    Load(#[from] UiLoadError),
    /// Binding-specific element or attribute structure was invalid.
    #[error("invalid binding document {path}: {message}")]
    Schema {
        /// Exact archive-relative source path.
        path: AssetPath,
        /// Rejected binding invariant.
        message: String,
    },
}
