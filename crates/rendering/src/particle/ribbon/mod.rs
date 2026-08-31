//! Stock M2 ribbon sampling, placement-local history, and strip preparation.

mod mesh;
mod pose;
mod trail;

pub use mesh::{M2RibbonMeshPlan, M2RibbonMeshPlanError, M2RibbonRenderVertex};
pub use pose::M2RibbonPose;
pub use trail::{M2RibbonControlPoint, M2RibbonSection, M2RibbonTrail, M2RibbonTrailError};
