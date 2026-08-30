//! Script regions, anchors, dimensions, clipping, and renderable region state.
//!
//! `CScriptRegion.cpp`, `CScriptRegionScript.cpp`, and `CSimpleRender.cpp`
//! establish the base region contract shared by textures, font strings, and
//! frame children.

mod c_script_region;
mod c_script_region_script;
mod status;

pub use c_script_region::{
    UiAnchor, UiDimensions, UiLayoutLayer, UiLayoutPlan, UiNodeLayout, UiPoint,
};
pub use status::UiLayoutError;
