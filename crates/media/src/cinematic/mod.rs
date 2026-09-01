//! Movie demuxing, decode, timing, and presentation handoff.
//!
//! `CSimpleMovieFrame.cpp`, the stock DivX decoder import, and FFmpeg-family
//! codec evidence establish this boundary. UI owns the frame widget while this
//! module owns decoded audio/video streams and their synchronization.

mod dependency;
mod status;
mod types;

pub use dependency::CinematicDecoder;
pub use status::CinematicError;
pub use types::CinematicVideoFrame;
