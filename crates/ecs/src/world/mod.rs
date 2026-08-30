//! Shipyard world ownership, entity lookup, and object lifecycle coordination.
//!
//! This module replaces the storage role evidenced by `ObjectMgrClient.cpp`
//! without reproducing the stock inheritance tree. Domain state stays in the
//! component modules and behavior stays in `solarity-systems`.

mod registry;
mod state;
mod types;

pub use state::{ActiveWorld, WorldStateError};
pub use types::{WorldBootstrap, WorldMapId};
