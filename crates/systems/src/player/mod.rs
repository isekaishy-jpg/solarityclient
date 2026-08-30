//! Local-player control, selection, interaction, and player-specific orchestration.
//!
//! Persistent player identity and replicated fields remain in `ecs::player`;
//! this cross-cutting seed is reserved for behavior applied to that state.

mod control;
mod interaction;
mod selection;
mod types;
