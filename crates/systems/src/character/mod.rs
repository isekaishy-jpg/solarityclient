//! Character creation, appearance, race/class, and model-composition behavior.

mod appearance;
mod character_creation;

pub use appearance::{UnitModelAppearance, UnitModelAppearanceError, resolve_unit_model};
