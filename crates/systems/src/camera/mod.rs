//! Player, world, and vehicle camera policy, transitions, constraints, and shake behavior.
//!
//! This cross-cutting seed coordinates ECS view state with renderer camera
//! projection without placing gameplay policy in either owning state crate.

mod controller;
mod transition;
mod types;
