//! Stock M2 ribbon sampling, placement-local history, and strip preparation.

mod mesh;
mod pose;
mod storage;
mod trail;
mod vertex;

pub use mesh::{M2RibbonMeshPlan, M2RibbonMeshPlanError};
pub use pose::M2RibbonPose;
pub use trail::{M2RibbonControlPoint, M2RibbonSection, M2RibbonTrail, M2RibbonTrailError};
pub use vertex::M2RibbonRenderVertex;
