//! Stable failures while translating resolved appearance rows into textures.

use solarity_asset::{AssetError, AssetPath};
use thiserror::Error;

use super::CharacterAtlasLayerKind;

/// A failure to construct an exact stock character texture plan.
#[derive(Debug, Error)]
pub enum CharacterTexturePlanError {
    /// A texture slot that stock requires for base composition is empty.
    #[error("character {kind} texture slot {slot} is required")]
    MissingTexture {
        /// Appearance layer whose DBC row is incomplete.
        kind: CharacterAtlasLayerKind,
        /// Zero-based `CharSections.dbc` texture-name column.
        slot: usize,
    },
    /// A nonempty DBC texture name violated archive-path invariants.
    #[error(transparent)]
    Asset(#[from] AssetError),
}

/// A failure while loading or compositing a planned character atlas.
#[derive(Debug, Error)]
pub enum CharacterTextureComposeError {
    /// Archive lookup, BLP parsing, or authored mip conversion failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// The starting authored mip required by stock does not exist.
    #[error("character texture {path} is missing authored mip {mip_level}")]
    MissingMip {
        /// Planned normalized BLP path.
        path: AssetPath,
        /// Zero-based authored mip level.
        mip_level: usize,
    },
    /// Stock would invoke its not-yet-recovered smaller-source scaler.
    #[error(
        "character texture {path} is {source_width}x{source_height}, below its {target_width}x{target_height} destination"
    )]
    SourceScalingRequired {
        /// Planned normalized BLP path.
        path: AssetPath,
        /// Authored top-mip width.
        source_width: u32,
        /// Authored top-mip height.
        source_height: u32,
        /// Stock destination width.
        target_width: u32,
        /// Stock destination height.
        target_height: u32,
    },
    /// A decoded authored mip cannot cover stock's requested source rectangle.
    #[error(
        "character texture {path} mip {mip_level} is {source_width}x{source_height}, outside requested region ({x}, {y}) {width}x{height}"
    )]
    SourceRegionOutOfBounds {
        /// Planned normalized BLP path.
        path: AssetPath,
        /// Zero-based authored mip level.
        mip_level: usize,
        /// Decoded source width.
        source_width: u32,
        /// Decoded source height.
        source_height: u32,
        /// Requested source left coordinate.
        x: u32,
        /// Requested source top coordinate.
        y: u32,
        /// Requested source width.
        width: u32,
        /// Requested source height.
        height: u32,
    },
}
