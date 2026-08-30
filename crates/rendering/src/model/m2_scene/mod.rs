//! Shared M2 mesh preparation and per-instance draw filtering.

mod gpu_state;
mod mesh;
mod status;
mod types;

pub use gpu_state::{M2DrawPushConstants, M2LocalLightState, M2MaterialUniform, M2SceneUniform};
pub use mesh::M2MeshPlan;
pub use status::M2MeshPlanError;
pub use types::{M2DrawCall, M2RenderVertex, M2TextureBinding};
