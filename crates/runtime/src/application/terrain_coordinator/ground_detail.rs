//! Worker-resident terrain-detail tables, first-profile meshes, and textures.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use solarity_asset::{
    AssetPath, AssetStore, BlpTextureCache, BlpTextureSource, DecodedTerrainTile,
    GroundEffectCatalog, M2ModelCache,
};
use solarity_rendering::{GroundDetailError, GroundDetailModel};

use super::{ResidentTerrainTile, RuntimeTerrainError};

/// Shares table decoding and immutable model preparation across tile jobs.
#[derive(Default)]
pub(super) struct GroundDetailAssetCache {
    catalog: Option<Arc<GroundEffectCatalog>>,
    models: BTreeMap<u32, Arc<GroundDetailModel>>,
}

/// Exact model/texture providers needed by the ground effects authored in one ADT.
#[derive(Default)]
pub(in crate::application) struct ResidentGroundDetailTile {
    pub(in crate::application) catalog: Option<Arc<GroundEffectCatalog>>,
    pub(in crate::application) models: BTreeMap<u32, Arc<GroundDetailModel>>,
    pub(in crate::application) textures: BTreeMap<AssetPath, Arc<BlpTextureSource>>,
}

impl GroundDetailAssetCache {
    /// Resolves layer references on the existing terrain asset worker.
    pub(super) fn prepare(
        &mut self,
        tile: &DecodedTerrainTile,
        models: &mut M2ModelCache,
        textures: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<ResidentGroundDetailTile, RuntimeTerrainError> {
        let effects = tile
            .chunks()
            .iter()
            .flat_map(|chunk| chunk.layers())
            .map(|layer| layer.effect_id())
            .filter(|id| *id != 0 && *id != u32::MAX)
            .collect::<BTreeSet<_>>();
        if effects.is_empty() {
            return Ok(ResidentGroundDetailTile::default());
        }
        let catalog = match &self.catalog {
            Some(catalog) => Arc::clone(catalog),
            None => {
                let catalog = Arc::new(GroundEffectCatalog::load(store)?);
                self.catalog = Some(Arc::clone(&catalog));
                catalog
            }
        };
        let ids = effects
            .into_iter()
            .filter_map(|id| catalog.texture(id))
            .flat_map(|effect| effect.models())
            .filter(|id| *id != 0)
            .collect::<BTreeSet<_>>();
        let mut resident = ResidentGroundDetailTile {
            catalog: Some(Arc::clone(&catalog)),
            ..Default::default()
        };
        for id in ids {
            let definition = catalog
                .doodad(id)
                .ok_or(GroundDetailError::MissingDoodad(id))?;
            if let std::collections::btree_map::Entry::Vacant(entry) = self.models.entry(id) {
                let model = models.load(store, definition.path())?;
                entry.insert(Arc::new(GroundDetailModel::prepare(&model)?));
            }
            let model = self
                .models
                .get(&id)
                .ok_or(GroundDetailError::MissingDoodad(id))?;
            if !resident.textures.contains_key(model.texture()) {
                resident.textures.insert(
                    model.texture().clone(),
                    textures.load(store, model.texture())?,
                );
            }
            resident.models.insert(id, Arc::clone(model));
        }
        Ok(resident)
    }
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
