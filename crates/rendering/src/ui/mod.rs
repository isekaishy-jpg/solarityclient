//! Renderer-owned CPU mesh and material vocabulary for stock UI quads.
//!
//! The `ui` crate resolves GlueXML state and ordering. This module owns the
//! acyclic renderer boundary: fixed vertex bytes, indexed ranges, and material
//! batches that can later be uploaded without reaching back into Lua state.

mod mesh;
mod status;
mod types;

pub use mesh::UiMeshPlan;
pub use status::UiMeshPlanError;
pub use types::{
    UiRenderBatch, UiRenderBlend, UiRenderQuad, UiRenderSource, UiRenderState, UiRenderTransform,
    UiRenderVertex, UiTextureAddressMode, UiTextureResidency,
};
