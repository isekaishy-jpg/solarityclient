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

/// A failure while loading stock default or saved binding assignments.
#[derive(Debug, Error)]
pub enum UiBindingAssignmentError {
    /// The selected default-binding archive member could not be read.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// A binding command file was not valid UTF-8.
    #[error("failed to decode binding assignment asset {path}: {message}")]
    TextEncoding {
        /// Exact archive-relative source path.
        path: AssetPath,
        /// Decoder context.
        message: String,
    },
    /// One line did not match the stock assignment command grammar.
    #[error("invalid binding assignment at line {line}: {message}")]
    Record {
        /// One-based source line.
        line: usize,
        /// Rejected command, token, or state invariant.
        message: String,
    },
}
