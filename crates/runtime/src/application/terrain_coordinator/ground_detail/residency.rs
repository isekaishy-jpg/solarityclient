//! Worker-resident terrain-detail tables, first-profile meshes, and textures.

use std::{collections::BTreeMap, sync::Arc};

use solarity_asset::{AssetPath, BlpTextureSource, DecodedTerrainTile, GroundEffectCatalog};
use solarity_rendering::GroundDetailModel;

use super::super::ResidentTerrainTile;

/// Shares table decoding and immutable model preparation across tile jobs.
#[derive(Default)]
pub(in super::super) struct GroundDetailAssetCache {
    pub(super) catalog: Option<Arc<GroundEffectCatalog>>,
    pub(super) models: BTreeMap<u32, Arc<GroundDetailModel>>,
}

/// Exact model/texture providers needed by the ground effects authored in one ADT.
#[derive(Default)]
pub(in crate::application) struct ResidentGroundDetailTile {
    pub(in crate::application) catalog: Option<Arc<GroundEffectCatalog>>,
    pub(in crate::application) models: BTreeMap<u32, Arc<GroundDetailModel>>,
    pub(in crate::application) textures: BTreeMap<AssetPath, Arc<BlpTextureSource>>,
}

impl ResidentTerrainTile {
    /// Borrows immutable asset providers prepared with this ADT generation.
    pub(in crate::application) const fn ground_detail(&self) -> &Arc<ResidentGroundDetailTile> {
        &self.ground_detail
    }

    /// Borrows authored terrain inputs for lazily generated nearby detail chunks.
    pub(in crate::application) const fn decoded(&self) -> &Arc<DecodedTerrainTile> {
        &self.decoded
    }
}
