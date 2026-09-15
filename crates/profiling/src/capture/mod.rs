//! Capture ownership keeps report I/O off game, audio and network threads.

mod controller;
mod trace_output;
mod writer;

pub use controller::Capture;
