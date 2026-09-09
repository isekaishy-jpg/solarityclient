//! Native ordinary and exterior portal projection without optional occlusion.

mod exterior;
mod frame;
mod polygon;
mod scene_camera;

pub use exterior::WorldModelExteriorPortalWindow;
pub use frame::WorldModelPortalProjectionFrame;
pub use polygon::WorldModelPortalProjector;
pub use scene_camera::WorldSceneCameraFrame;
