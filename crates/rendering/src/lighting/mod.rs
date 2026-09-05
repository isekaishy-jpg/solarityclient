//! World, model, and local-light data prepared for GPU consumption.
//!
//! This boundary follows `M2Light.cpp`, `Lightning.cpp`, `WorldScene.cpp`, and
//! the light-bearing asset formats. ECS retains gameplay state; rendering owns
//! frame-local light preparation.

mod m2_light;
mod m2_light_override;
mod world_scene;

pub use m2_light::{
    M2DirectionalLight, M2PointLight, M2Sunlight, glue_character_sunlight, glue_ghost_sunlight,
    merge_wotlk_directional_lights,
};
pub use m2_light_override::M2LightOverride;
