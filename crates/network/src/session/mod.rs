//! Login and world-session state machines, dispatch, and orderly shutdown.
//!
//! The boundary follows `NetClient.cpp`, `NetInternal.cpp`, and
//! `ClientServices.cpp`. Every async task has session ownership; no task may be
//! detached from connection shutdown.

mod error;
mod net_client;
mod net_internal;

pub use error::{WorldSessionError, WorldSessionStage};
