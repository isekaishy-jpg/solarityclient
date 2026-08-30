//! Render-facing joins across build-12340 display and customization tables.
//!
//! The raw catalogs retain authored database rows. This module performs the
//! exact cross-table and filename transformations used when stock constructs
//! one M2 character component, without loading model or texture bytes.

mod character;
mod creature;
mod status;

pub use character::{
    CharacterCustomization, CharacterGeosetSelection, CharacterModelAppearance,
    CharacterSectionKind,
};
pub use creature::CreatureModelAppearance;
pub use status::AppearanceError;
