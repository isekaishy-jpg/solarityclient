//! Stable failures produced while joining valid build-12340 appearance tables.

use thiserror::Error;

use super::CharacterSectionKind;

/// A missing reference or customization key required to construct an M2 appearance.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AppearanceError {
    /// `UNIT_FIELD_DISPLAYID` does not name a creature display row.
    #[error("creature display {display_id} is absent")]
    MissingCreatureDisplay {
        /// The unresolved `CreatureDisplayInfo.dbc` identifier.
        display_id: u32,
    },
    /// A creature display references no model metadata row.
    #[error("creature display {display_id} references absent model {model_id}")]
    MissingCreatureModel {
        /// The display whose foreign key failed.
        display_id: u32,
        /// The unresolved `CreatureModelData.dbc` identifier.
        model_id: u32,
    },
    /// A model metadata row intentionally or erroneously carries no M2 path.
    #[error("creature model {model_id} for display {display_id} has no model path")]
    MissingCreatureModelPath {
        /// The display being resolved.
        display_id: u32,
        /// The model row with an empty path.
        model_id: u32,
    },
    /// A nonzero extended-display foreign key does not name a row.
    #[error("creature display {display_id} references absent extra appearance {extra_id}")]
    MissingCreatureDisplayExtra {
        /// The display whose foreign key failed.
        display_id: u32,
        /// The unresolved `CreatureDisplayInfoExtra.dbc` identifier.
        extra_id: u32,
    },
    /// The supplied player customization does not name a stock texture section.
    #[error(
        "character race {race_id} gender {gender_id} has no {kind} section at variation {variation_index} color {color_index}"
    )]
    MissingCharacterSection {
        /// `ChrRaces.dbc` identifier.
        race_id: u32,
        /// Stock gender identifier.
        gender_id: u32,
        /// Component variation category.
        kind: CharacterSectionKind,
        /// Requested customization variation.
        variation_index: u32,
        /// Requested customization color.
        color_index: u32,
    },
}
