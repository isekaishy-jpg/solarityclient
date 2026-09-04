//! Batched UI mesh generation and renderer-facing presentation submission.

mod c_simple_render;
mod ordered_quads;
mod presentation;
mod resources;
mod status;

pub use c_simple_render::UiRenderPlan;
pub use presentation::{
    UiModelFog, UiModelLight, UiModelLightSets, UiModelPresentation, UiPresentationPacket,
    UiPresentationPacketKey, UiPresentationPlan, UiTexturePresentation, UiTextureSource,
};
pub use resources::{UiTextureAssetBindings, UiTextureAssetPlan, UiTextureAssetRequest};
pub use status::UiRenderError;
