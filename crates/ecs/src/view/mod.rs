//! Renderer-independent local view state, including camera target, mode, zoom, and orientation.
//!
//! Stock camera work crosses `Camera.cpp`, `VehicleCamera_C.cpp`, player state,
//! and world presentation. This module owns only durable ECS-facing view state;
//! camera behavior belongs to `systems` and GPU projection belongs to `rendering`.

mod state;
mod types;

pub use state::PlayerViewState;
