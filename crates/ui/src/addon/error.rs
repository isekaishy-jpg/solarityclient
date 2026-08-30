//! AddOn discovery and TOC decoding failures.

use solarity_asset::AssetError;
use thiserror::Error;

/// A failure while constructing the stock AddOn catalog.
#[derive(Debug, Error)]
pub enum AddonCatalogError {
    /// The underlying permitted AddOn source stack failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Build-12340 TOC metadata is byte-oriented but all shipped metadata is
    /// representable as UTF-8 for the locales accepted by this implementation.
    #[error("AddOn {addon} TOC is not valid UTF-8: {message}")]
    InvalidEncoding {
        /// AddOn folder identity.
        addon: String,
        /// Decoder context.
        message: String,
    },
    /// A numeric or boolean metadata value could not be interpreted.
    #[error("AddOn {addon} has invalid {field} metadata {value:?}")]
    InvalidMetadata {
        /// AddOn folder identity.
        addon: String,
        /// TOC field name.
        field: &'static str,
        /// Rejected value.
        value: String,
    },
}
