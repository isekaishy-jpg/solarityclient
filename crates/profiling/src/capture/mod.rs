//! Capture ownership keeps report I/O off game, audio and network threads.

mod controller;
mod writer;

pub use controller::Capture;
