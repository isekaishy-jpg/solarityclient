//! Native registration destinations, results and errors.

use super::super::RuntimeWorldModelMovementOwner;
use solarity_asset::{TerrainChunkIndex, TerrainTileIndex};
use solarity_systems::{
    MovementCollectionError, TerrainCollisionError, WorldModelCollisionError,
    WorldModelRegistrationSelection,
};
use thiserror::Error;

/// One native list destination, scoped to the registration query's map.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeMovementReference {
    /// The dynamic list following an MCNK's authored M2 references.
    Terrain {
        /// Owning ADT.
        tile: TerrainTileIndex,
        /// Owning MCNK.
        chunk: TerrainChunkIndex,
    },
    /// The dynamic list following a WMO group's authored M2 references.
    WorldModel {
        /// MODF identity shared across resident ADTs.
        unique_id: u32,
        /// Root MOGI/group index.
        group: usize,
    },
    /// A group list owned by an admitted replicated WMO root.
    GameObjectWorldModel {
        /// Exact replicated lifetime of the root.
        identity: solarity_ecs::WorldObjectIdentity,
        /// Root MOGI/group index.
        group: usize,
    },
}

/// Invalid geometry or an unresolved reference in an admitted generation.
#[derive(Debug, Error)]
pub enum RuntimeMovementRegistrationError {
    /// Camera-root portal projection or scene traversal failed.
    #[error(transparent)]
    SceneVisibility(#[from] solarity_systems::WorldModelVisibilityError),
    /// Outdoor model depth cannot be represented by the native scene camera.
    #[error(transparent)]
    SceneDepth(#[from] solarity_systems::WorldSceneDepthError),
    /// An attached MODD could not form its current placement.
    #[error(transparent)]
    M2(#[from] solarity_systems::M2CollisionError),
    /// Terrain point or height selection failed.
    #[error(transparent)]
    Terrain(#[from] TerrainCollisionError),
    /// WMO floor/portal selection failed.
    #[error(transparent)]
    WorldModel(#[from] WorldModelCollisionError),
    /// Native box transformation failed.
    #[error(transparent)]
    Collection(#[from] MovementCollectionError),
    /// A queried liquid group references unavailable behavior data.
    #[error(transparent)]
    Liquid(#[from] solarity_systems::SubmergedLiquidError),
    /// An admitted MODF no longer resolves to its complete collision generation.
    #[error("resident movement registration reference is invalid")]
    InvalidReference,
}

/// Reusable result and scratch for one GameObject model's spatial registration.
///
/// References are emitted in native allocation order. The caller inserts each
/// GameObject at the front of its destination's dynamic list. Pending and failed
/// registrations clear all results so a former placement cannot remain active.
#[derive(Default)]
pub struct RuntimeMovementRegistrationQuery {
    pub(super) map_id: Option<u32>,
    pub(super) references: Vec<RuntimeMovementReference>,
    pub(super) selection: Option<WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>>,
    pub(super) groups: Vec<usize>,
}

impl RuntimeMovementRegistrationQuery {
    /// Creates retained registration storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the map of the last complete registration.
    #[must_use]
    pub const fn map_id(&self) -> Option<u32> {
        self.map_id
    }

    /// Returns destinations in native reference-allocation order.
    #[must_use]
    pub fn references(&self) -> &[RuntimeMovementReference] {
        &self.references
    }

    /// Retains native floor/fallback channels for interior and support state.
    #[must_use]
    pub const fn selection(
        &self,
    ) -> Option<WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>> {
        self.selection
    }

    pub(in crate::application::terrain_coordinator::movement) fn clear(&mut self) {
        self.map_id = None;
        self.references.clear();
        self.selection = None;
        self.groups.clear();
    }
}

/// WMO identity and location gates, before resolving any DBC relationships.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) struct UnitWorldModelLocation {
    pub key: solarity_asset::WorldModelAreaKey,
    pub world_model_only: bool,
    /// 782560 excludes transformed roots when overriding the terrain AreaTable ID.
    pub area_override: bool,
}
