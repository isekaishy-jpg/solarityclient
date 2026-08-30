//! Shared WMO surface mesh preparation before placed scene publication.

mod mesh;
mod placement;
mod status;
mod types;

pub use mesh::WorldModelMeshPlan;
pub use placement::PlacedWorldModelDrawPlan;
pub use status::{WorldModelMeshPlanError, WorldModelPlacementError};
pub use types::{WorldModelDrawCall, WorldModelGroupRange, WorldModelRenderVertex};
