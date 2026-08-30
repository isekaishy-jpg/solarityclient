//! Applies movement updates, interpolation, and trajectory transitions.
//!
//! This behavior is separated from movement components by the stock
//! `Movement.cpp`, `Movement_C.cpp`, and `MovementShared.cpp` family.

mod movement_shared;
mod movement_source;
mod path;
