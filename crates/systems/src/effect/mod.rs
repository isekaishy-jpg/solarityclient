//! Advances transient object, spell, and visual-effect lifecycles.
//!
//! `ObjectEffect.cpp`, `Effect_C.cpp`, and `SpellVisuals.cpp` provide the stock
//! responsibility cluster. Renderer synchronization is emitted through a
//! boundary rather than mutating rendering internals.

mod model_scale;
mod object_effect;
mod unit_tint;
mod unit_water;
pub use unit_tint::UnitModelTint;

pub use model_scale::{UnitEffectScale, unit_world_effect_factor};
pub use unit_water::{
    UnitBreathEnvironment, UnitBreathState, UnitWaterEffect, UnitWaterSprayInput,
    unit_player_inebriation,
};
