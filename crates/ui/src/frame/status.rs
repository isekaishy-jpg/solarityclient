//! Stable failures while registering stock UI object declarations.

use solarity_asset::AssetPath;
use thiserror::Error;

/// A failure while constructing the global template and root-object catalog.
#[derive(Debug, Error)]
pub enum UiObjectError {
    /// A root XML object violated stock registration invariants.
    #[error("invalid UI object declaration in {path}: {message}")]
    Declaration {
        /// XML source containing the declaration.
        path: AssetPath,
        /// Type, name, boolean, or inheritance context.
        message: String,
    },
}

/// A failure while decoding frame ordering or interaction properties.
#[derive(Debug, Error)]
pub enum UiFrameError {
    /// A frame property violated the observed stock XML vocabulary.
    #[error("invalid UI frame property in {path}: {message}")]
    Property {
        /// XML source containing the declaration.
        path: AssetPath,
        /// Attribute and value context.
        message: String,
    },
}
