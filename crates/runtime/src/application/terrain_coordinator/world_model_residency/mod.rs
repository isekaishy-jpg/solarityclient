//! WMO source residency and ordered nested resource preparation.

mod cache;
mod materials;
mod preparation;
mod scene;
mod source;

pub(in crate::application) use cache::ResidentWorldModelCache;
pub(in crate::application) use materials::{
    ResidentWorldModelMaterialTextures, ResidentWorldModelTexture,
};
pub(super) use preparation::{WorldModelPreparation, prepare_global_world_model};
pub(super) use scene::same_world_model_placement;
pub(in crate::application) use scene::{ResidentWorldModelPlacement, ResidentWorldModelScene};
pub(in crate::application) use source::{ResidentWorldModelSource, WorldModelSourcePreparation};
