//! Caller-driven construction checkpoints; no executor owns the Lua state.
//!
//! Pinned local futures retain source-plan borrows across caller polls. Every
//! authored callback remains indivisible; cancellation releases the local graph.

mod budget;
mod task;

pub(crate) use budget::StartupBudget;
pub(crate) use task::{StartupTask, complete_unyielding};

#[cfg(test)]
#[path = "../../tests/unit/startup.rs"]
mod construction_tests;
