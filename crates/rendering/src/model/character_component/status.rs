//! Stable failures while translating resolved appearance rows into textures.

use solarity_asset::AssetError;
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
