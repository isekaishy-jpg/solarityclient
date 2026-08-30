//! Bounded worker-pool configuration and lifecycle.
//!
//! This module wraps the threading responsibility visible in the stock
//! `SThread.cpp` family. It owns Rayon pool creation and shutdown, not network
//! async execution or detached background work.

mod executor;
mod task;
mod types;
mod worker;
