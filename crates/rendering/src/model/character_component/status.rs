//! Stable failures while translating resolved appearance rows into textures.

use solarity_asset::{AssetError, AssetPath};
use thiserror::Error;

use super::CharacterAtlasLayerKind;

/// A failure while selecting visible character M2 geosets.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum CharacterGeosetPlanError {
    /// A helmet visibility row cannot select a non-player gender column.
    #[error("character gender {gender_id} cannot select helmet geoset visibility")]
    UnsupportedHelmetGender {
        /// Gender identifier resolved from the character appearance tables.
        gender_id: u32,
    },
}

/// A failure while translating stock item display names into attachment paths.
#[derive(Debug, Error)]
pub enum CharacterAttachmentPlanError {
    /// A caller supplied a record outside the four trailing enumeration bags.
    #[error("character-enumeration bag slot {bag_slot} is outside 19 through 22")]
    InvalidCharacterEnumerationBagSlot {
        /// Rejected wire-order slot.
        bag_slot: u8,
    },
    /// A held slot was constructed from display-only NPC armor data.
    #[error("held equipment slot {slot:?} has no Item.dbc definition")]
    MissingItemDefinition {
        /// Public slot that requires weapon category and sheath metadata.
        slot: solarity_ecs::PlayerEquipmentSlot,
    },
    /// A helmet model cannot select a suffix outside stock's two genders.
    #[error("character gender {gender_id} cannot select a helmet model suffix")]
    UnsupportedHelmetGender {
        /// Gender identifier resolved from the character appearance tables.
        gender_id: u32,
    },
    /// A ranged public slot contains no stock ranged weapon subclass.
    #[error("item class {class_id} subclass {subclass_id} has no ranged attachment hand")]
    UnsupportedRangedItem {
        /// `Item.dbc` class identifier.
        class_id: u32,
        /// Class-local weapon subclass identifier.
        subclass_id: u32,
    },
    /// A nonempty DBC model or texture name violated archive-path invariants.
    #[error(transparent)]
    Asset(#[from] AssetError),
}

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
    /// A missing universal item texture requires an unsupported sex suffix.
    #[error("character gender {gender_id} cannot select an equipment texture suffix")]
    UnsupportedEquipmentGender {
        /// Gender identifier resolved from the character appearance tables.
        gender_id: u32,
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
    /// Source dimensions cannot enter stock's exact one-level scaler.
    #[error(
        "character texture {path} is {source_width}x{source_height}, which cannot scale exactly once into its {target_width}x{target_height} destination"
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
