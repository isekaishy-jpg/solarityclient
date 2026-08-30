//! Batched UI mesh generation and renderer-facing presentation submission.

mod c_simple_render;
mod presentation;

pub use presentation::{
    UiPresentationPacket, UiPresentationPacketKey, UiPresentationPlan, UiTexturePresentation,
    UiTextureSource,
};
