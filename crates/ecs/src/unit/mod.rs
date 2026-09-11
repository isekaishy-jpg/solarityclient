//! Unit, creature-stat, combat-state, and combat-log components.
//!
//! This module follows `Unit_C.cpp`, `CreatureStats.cpp`, `UnitCombat_C.cpp`,
//! and `UnitCombatLog_C.cpp`. It stores authoritative client state but does not
//! execute combat rules.

mod unit_attack;
mod unit_aura;
pub use unit_aura::{UnitAura, UnitAuras};
mod unit_c;
mod unit_flags;
mod unit_presentation;
mod unit_stats;
mod unit_virtual_items;
mod unit_vitals;

pub use unit_attack::UnitAttackTarget;
pub use unit_c::UnitIdentity;
pub use unit_flags::UnitFlags;
pub use unit_presentation::{UnitAnimationTier, UnitPresentation, UnitSheathState};
pub use unit_stats::{UNIT_PRIMARY_STAT_COUNT, UnitStats};
pub use unit_virtual_items::UnitVirtualItems;
pub use unit_vitals::{UnitHealthPrediction, UnitVitals};
