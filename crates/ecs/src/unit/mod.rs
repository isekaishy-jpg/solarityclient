//! Unit, creature-stat, combat-state, and combat-log components.
//!
//! This module follows `Unit_C.cpp`, `CreatureStats.cpp`, `UnitCombat_C.cpp`,
//! and `UnitCombatLog_C.cpp`. It stores authoritative client state but does not
//! execute combat rules.

mod unit_c;
mod unit_flags;
mod unit_presentation;
mod unit_vitals;

pub use unit_c::UnitIdentity;
pub use unit_flags::UnitFlags;
pub use unit_presentation::UnitPresentation;
pub use unit_vitals::UnitVitals;
