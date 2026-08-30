//! Advances transient object, spell, and visual-effect lifecycles.
//!
//! `ObjectEffect.cpp`, `Effect_C.cpp`, and `SpellVisuals.cpp` provide the stock
//! responsibility cluster. Renderer synchronization is emitted through a
//! boundary rather than mutating rendering internals.

mod object_effect;
