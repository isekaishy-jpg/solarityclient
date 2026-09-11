//! Original MapShadow.cpp quality policy and primary-map projections.

mod environment;
mod projection;
mod quality;

pub use environment::{
    WorldEnvironmentShadowMap, WorldEnvironmentShadowState, WorldEnvironmentShadowUpdate,
};
pub use projection::{WorldShadowProjection, WorldShadowProjectionError};
pub use quality::WorldShadowQuality;
