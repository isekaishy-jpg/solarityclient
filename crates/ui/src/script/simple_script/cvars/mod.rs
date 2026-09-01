//! Stock console-variable definitions and script-visible mutable state.

mod definitions;
mod registry;

pub(super) use registry::{UiCVarRegistry, UiCVarSetError};
