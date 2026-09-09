//! Player-volume contact queries recovered from build-12340 `Collide.cpp`.

mod cache;
mod face;
mod fall;
mod ground;
mod ground_normal;
mod polygon;
mod response;
mod support;
mod sweep;
mod volume;

pub use fall::MovementFallContactKind;
pub use support::MovementSupportProfile;
pub use sweep::{MovementCollisionTriangle, MovementSweep, MovementSweepError};
pub use volume::{MovementCollisionPlane, MovementCollisionVolume};

// Exact binary constants, rather than rounded geometry tolerances. The first
// four live at 0x00A37F14, 0x009F1224, 0x009EA27C, and 0x00A3F854.
const CONTACT_TOLERANCE: f32 = f32::from_bits(0x3ab6_0b61);
const DEGENERATE_TOLERANCE: f32 = f32::from_bits(0x3580_0000);
const DIRECTION_TOLERANCE: f32 = f32::from_bits(0x3480_0000);
const MINIMUM_SWEEP_LENGTH: f32 = f32::from_bits(0x3ce3_8e39);
