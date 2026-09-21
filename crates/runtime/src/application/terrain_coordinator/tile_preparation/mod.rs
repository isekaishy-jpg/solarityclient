//! Complete ADT generations assembled through ordered, resumable asset stages.

mod doodads;
mod preparation;
mod shared;

pub(super) use preparation::TilePreparation;
pub(in crate::application) use shared::{PendingWorldModel, SharedTerrainSources};
