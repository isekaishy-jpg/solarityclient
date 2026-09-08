//! Applies combat-state and combat-log transitions to ECS units.
//!
//! The system boundary follows `UnitCombat_C.cpp` and `UnitCombatLog_C.cpp`.
//! Server messages remain authoritative; this module must not simulate server
//! outcomes as a fallback for absent protocol state.

mod classification;
mod unit_combat_c;
mod unit_combat_log_c;

pub use classification::{
    CombatLogObjectClassification, combat_log_object_flags, faction_template_reaction,
    is_player_guid,
};

pub use unit_combat_c::predict_unit_health;
pub use unit_combat_log_c::EnvironmentalDamageKind;
