//! Native 7AC060 traversal after each portal's polygon has been projected.

mod camera_scene;
mod projection;
mod query;

pub use camera_scene::WorldModelCameraSceneQuery;
pub use projection::{
    WorldModelExteriorPortalWindow, WorldModelPortalProjectionFrame, WorldModelPortalProjector,
    WorldSceneCameraFrame, WorldSceneFrustum,
};
pub use query::WorldModelVisibilityQuery;

use thiserror::Error;

/// Invalid input to the retained WMO portal visibility traversal.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum WorldModelVisibilityError {
    /// A camera group is outside the admitted WMO generation.
    #[error("world-model visibility group is outside the admitted model")]
    InvalidGroup,
    /// The projection array must contain one entry per authored portal.
    #[error("world-model visibility portal projection count does not match the model")]
    PortalCount,
    /// Camera coordinates or projected screen coordinates are not finite.
    #[error("world-model visibility coordinates are not finite")]
    NonFiniteCoordinates,
    /// The supplied frustum corners cannot form finite clipping planes.
    #[error("world-model visibility frustum has degenerate clipping faces")]
    DegenerateFrustum,
    /// Perspective parameters cannot form the ordinary scene projection.
    #[error("world-model scene camera projection is invalid")]
    InvalidCameraProjection,
    /// Authored group bounds or their placed world transform are invalid.
    #[error(transparent)]
    Bounds(#[from] crate::collision::MovementCollectionError),
}

/// One ordered native group visit, including the inherited fog bank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelVisibilityVisit {
    /// Authored group index.
    pub group: usize,
    /// True selects the indoor model fog bank.
    pub indoor_fog: bool,
    /// Number of traversed portals from this query's initial group.
    pub depth: u32,
    /// Native min-Y, min-X, max-Y, max-X projection bounds for this visit.
    pub screen_window: [f32; 4],
}

/// Ordered camera-root scene callbacks before graphics and unit processing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldModelSceneVisibilityEvent {
    /// Visit a resident group with the inherited fog bank and clipping window.
    Group(WorldModelVisibilityVisit),
    /// First eligible 7A8F20 encounter for this portal in the root traversal.
    /// The reference identifies the side used for displaced projection. An
    /// encounter remains consumed even when that projection rejects the polygon.
    ExteriorPortal {
        /// Index in the admitted root's authored portal-reference array.
        reference: usize,
    },
}
