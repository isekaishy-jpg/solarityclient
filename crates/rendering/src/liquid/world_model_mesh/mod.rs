//! Native WMO liquid grid, portal clipping, and triangle-strip preparation.

mod clipping;
mod geometry;
mod plan;

pub use plan::{
    WorldModelLiquidDepthColumn, WorldModelLiquidMeshError, WorldModelLiquidMeshPlan,
    WorldModelLiquidSurface,
};
