//! Shared M2 mesh preparation and per-instance draw filtering.

mod gpu_state;
mod liquid;
mod mesh;
mod order;
mod shadow;
mod shadow_material;
mod status;
mod types;

pub use gpu_state::{M2DrawPushConstants, M2LocalLightState, M2MaterialUniform, M2SceneUniform};
pub use liquid::{M2LiquidPasses, M2LiquidState};
pub use mesh::M2MeshPlan;
pub use order::{
    M2EffectOrder, M2ElementAlphaState, M2TransparentPass, M2TransparentSortKey,
    compare_m2_transparent, m2_model_distance_key, m2_section_distance_key,
};
pub use shadow::{M2ShadowMatrix, M2ShadowState};
pub use shadow_material::M2ShadowMaterial;
pub use status::M2MeshPlanError;
pub use types::{M2DrawCall, M2RenderVertex, M2TextureBinding};
