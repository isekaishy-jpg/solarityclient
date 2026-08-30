//! Transition from authenticated realm selection to the world connection.
//!
//! `WowConnection.cpp` provides direct stock evidence for this boundary. It
//! coordinates transport and session state without embedding socket mechanics
//! or packet definitions.

mod wow_connection;
