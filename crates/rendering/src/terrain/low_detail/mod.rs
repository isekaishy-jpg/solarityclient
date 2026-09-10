//! Map-wide native WDL mesh storage and the far-terrain frame projection.

mod frame;
mod mesh;
mod scale;

pub use frame::{TerrainLowDetailMap, WorldLowDetailFrame};
pub use mesh::TerrainLowDetailMesh;
pub use scale::WorldHorizonScale;
