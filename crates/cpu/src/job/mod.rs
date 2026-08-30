//! Owned CPU work units and their completion contracts.
//!
//! Jobs describe CPU-bound work without exposing Rayon's concrete types to
//! domain crates. Every submitted job must retain an owner and an observable
//! completion or cancellation path.

mod types;
