//! Runtime event scheduling, timers, and lifecycle-bound callbacks.
//!
//! `EvtSched.cpp` and `EvtTimer.cpp` establish this stock boundary. Tokio owns
//! async timing primitives while this module owns client event semantics and
//! cancellation.

mod evt_sched;
mod evt_timer;
mod s_evt;
