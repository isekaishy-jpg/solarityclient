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
