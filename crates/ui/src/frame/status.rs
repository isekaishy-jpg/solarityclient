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
    /// A virtual child depends on a template registered by a later AddOn.
    #[error("UI object {object} in {path} awaits template {template}")]
    UnavailableTemplate {
        /// XML source containing the deferred child.
        path: AssetPath,
        /// Expanded or authored object name.
        object: String,
        /// Template not yet registered in the current catalog.
        template: String,
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
    /// The constructed ownership graph could not produce stock startup state.
    #[error("cannot resolve UI frame state: {message}")]
    Resolution {
        /// Ownership, index, or level context.
        message: String,
    },
}
