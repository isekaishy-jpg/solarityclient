//! Transition from authenticated realm selection to the world connection.
//!
//! `WowConnection.cpp` provides direct stock evidence for this boundary. It
//! coordinates transport and session state without embedding socket mechanics
//! or packet definitions.

mod error;
mod world_state;
mod wow_connection;

pub use error::{WorldAuthError, WorldAuthFailure, WorldAuthStage};
pub use world_state::{
    AccountExpansion, WorldAuthProgress, WorldQueue, WorldSession, WorldSessionInfo,
};
pub use wow_connection::WorldConnection;
