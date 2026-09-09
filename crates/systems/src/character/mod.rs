//! Character creation, appearance, race/class, and model-composition behavior.

mod appearance;
mod body_scale;
mod character_creation;

pub use appearance::{UnitModelAppearance, UnitModelAppearanceError, resolve_unit_model};
pub use body_scale::resolve_unit_body_scale;
