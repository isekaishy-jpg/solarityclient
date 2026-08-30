//! Exact build-12340 M2 shader substitution and material render state.

mod material;
mod selector;
mod status;
mod types;

pub use material::{M2BlendFactor, M2MaterialState};
pub use selector::M2ShaderPlan;
pub use status::M2ShaderPlanError;
pub use types::{M2PixelShader, M2VertexShader};
