//! Native ordinary and exterior portal projection without optional occlusion.

mod exterior;
mod frame;
mod polygon;

pub use exterior::WorldModelExteriorPortalWindow;
pub use frame::WorldModelPortalProjectionFrame;
pub use polygon::WorldModelPortalProjector;
