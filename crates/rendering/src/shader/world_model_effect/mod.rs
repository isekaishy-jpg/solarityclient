//! Exact build-12340 MapObj effect, material, and surface-pass state.

mod material;
mod pass;

pub use material::{
    WorldModelBlendFactor, WorldModelBlendState, WorldModelFogMode, WorldModelMaterialState,
};
pub use pass::{WorldModelLightingMode, WorldModelSurfacePass, WorldModelSurfacePassPlan};
