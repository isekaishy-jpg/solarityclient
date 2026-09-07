//! Ordered resident movement geometry selection from build 12340.

mod bounds;
mod liquid;
mod terrain_grid;
mod triangle;
mod world_model;

use super::MovementSweepError;
use thiserror::Error;

pub use bounds::MovementCollisionBounds;
pub use liquid::append_terrain_liquid_movement;
pub use terrain_grid::MovementTerrainChunks;
pub(super) use terrain_grid::terrain_square_bounds;
pub(super) use triangle::{calculated_triangle, transform_point};
use triangle::{transformed_query, world_model_triangle};
pub use world_model::MovementBspCacheMode;
pub(super) use world_model::MovementBspQuery;
pub(super) use world_model::cached_leaf_eligibility;

/// Invalid query geometry at the resident movement-collision boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MovementCollectionError {
    /// Query bounds are non-finite or reversed.
    #[error("movement collision bounds are invalid")]
    InvalidBounds,
    /// The requested terrain box falls outside stock's map-coordinate domain.
    #[error("movement collision bounds are outside the terrain map")]
    OutsideTerrainMap,
    /// An interior registration names no group in the admitted root.
    #[error("world-model registration group {group} is outside the admitted model")]
    InvalidRegistrationGroup {
        /// Invalid root group index.
        group: usize,
    },
    /// The source BSP contains a cycle.
    #[error("movement collision BSP contains a cycle")]
    CyclicBsp,
    /// Stock's per-group face selection capacity was exceeded.
    #[error("movement collision exceeded the stock WMO face selection capacity")]
    WorldModelFaceLimit,
    /// A selected source face cannot form a finite collision triangle.
    #[error(transparent)]
    Triangle(#[from] MovementSweepError),
}
