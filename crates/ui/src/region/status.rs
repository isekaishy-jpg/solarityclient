//! Stable failures while decoding stock region layout properties.

use solarity_asset::AssetPath;
use thiserror::Error;

/// A failure while converting XML region properties into typed layout data.
#[derive(Debug, Error)]
pub enum UiLayoutError {
    /// A stock layout value or structure was invalid.
    #[error("invalid UI layout in {path}: {message}")]
    Layout {
        /// XML source containing the invalid value.
        path: AssetPath,
        /// Element, attribute, number, or name-expansion context.
        message: String,
    },
}
