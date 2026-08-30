//! Stable failures at the archive-backed font boundary.

use solarity_asset::{AssetError, AssetPath};
use thiserror::Error;

/// A failure while owning a stock font face or rasterizing a glyph.
#[derive(Debug, Error)]
pub enum FontError {
    /// The selected font asset could not be resolved or read.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// FreeType could not initialize its library owner.
    #[error("failed to initialize FreeType: {message}")]
    Library {
        /// Dependency context without exposing its error type.
        message: String,
    },
    /// The selected archive bytes were not a usable font face.
    #[error("failed to open font face {path}: {message}")]
    Face {
        /// The normalized archive path.
        path: AssetPath,
        /// FreeType context.
        message: String,
    },
    /// FreeType rejected the requested pixel height.
    #[error("failed to set font {path} to {pixel_height}px: {message}")]
    PixelSize {
        /// The normalized archive path.
        path: AssetPath,
        /// Requested pixel height.
        pixel_height: u32,
        /// FreeType context.
        message: String,
    },
    /// A character could not be loaded and rendered.
    #[error("failed to rasterize {character:?} from font {path}: {message}")]
    Glyph {
        /// The normalized archive path.
        path: AssetPath,
        /// Requested Unicode scalar value.
        character: char,
        /// FreeType context.
        message: String,
    },
    /// The returned bitmap cannot be represented as an 8-bit coverage image.
    #[error("unsupported glyph bitmap from font {path}: {message}")]
    Bitmap {
        /// The normalized archive path.
        path: AssetPath,
        /// Structural or pixel-format context.
        message: String,
    },
    /// A stock `<Font>` object was structurally invalid.
    #[error("invalid font definition in {path}: {message}")]
    Definition {
        /// The XML source containing the definition.
        path: AssetPath,
        /// Structural, value, or inheritance context.
        message: String,
    },
}
