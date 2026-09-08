//! Applies spell-cast and missile state transitions from authoritative events.
//!
//! The boundary follows `Spell_C.cpp`, `SpellCast.cpp`, and `Missile_C.cpp` and
//! intentionally excludes UI spell-book behavior.

mod aura;
mod spell_cast;
pub use aura::unit_has_aura_type;
mod spell_visuals;
