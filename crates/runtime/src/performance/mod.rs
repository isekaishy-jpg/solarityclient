//! Frame-performance measurement independent of presentation backends.

mod frame_rate;
mod limiter;

pub use frame_rate::FrameRateCounter;
pub(crate) use limiter::FrameLimiter;
