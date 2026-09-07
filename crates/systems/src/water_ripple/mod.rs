//! Stock unit wake emission and the retained surface-ripple lifecycle.

mod emission;
mod envelope;
mod pool;

pub use emission::{WaterRippleClock, WaterRippleEmission, WaterRippleError, WaterRippleUnit};
pub use envelope::{WaterRippleEnvelope, WaterRippleEnvelopeError};
pub use pool::{WaterRipple, WaterRippleOwner, WaterRipplePool};
