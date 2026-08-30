//! Scriptable texture and embedded-texture widget behavior recovered from stock RTTI.

mod state;
mod types;

pub use state::{UiTextureState, UiTextureStatePlan};
pub use types::{
    UiBlendMode, UiGradientOrientation, UiTexCoords, UiTextureColor, UiTextureError, UiTextureFile,
    UiTextureGradient, UiTextureLayer, UiTextureNode, UiTexturePlan,
};

pub(crate) use types::canonical_texture_asset;
