//! Persistent F10 diagnostics shared by every subsystem and worker thread.

mod control;

pub(super) use control::RuntimeInstrumentation;
