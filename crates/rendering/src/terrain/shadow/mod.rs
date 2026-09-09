//! Original MapShadow.cpp quality policy and primary-map projections.

mod projection;
mod quality;

pub use projection::{WorldShadowProjection, WorldShadowProjectionError};
pub use quality::WorldShadowQuality;
