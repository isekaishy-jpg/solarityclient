//! Exact build-12340 M2 shader substitution and material render state.

mod material;
mod permutation;
mod selector;
mod status;
mod types;

pub use material::{M2BlendFactor, M2MaterialState};
pub use permutation::{
    M2LocalLightCount, M2ShaderPermutation, M2ShadowFiltering, M2ShadowPermutation,
};
pub use selector::M2ShaderPlan;
pub use status::M2ShaderPlanError;
pub use types::{M2PixelShader, M2VertexShader};
