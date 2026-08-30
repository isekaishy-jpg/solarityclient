//! World, model, and local-light data prepared for GPU consumption.
//!
//! This boundary follows `M2Light.cpp`, `Lightning.cpp`, `WorldScene.cpp`, and
//! the light-bearing asset formats. ECS retains gameplay state; rendering owns
//! frame-local light preparation.

mod m2_light;
mod world_scene;
