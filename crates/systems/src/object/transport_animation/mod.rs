//! Type-11 transport animation phase and authored geometry.

mod clock;
mod track;

pub use clock::TransportAnimationClock;
pub use track::{TransportAnimationError, TransportAnimationSample, TransportAnimationTrack};
