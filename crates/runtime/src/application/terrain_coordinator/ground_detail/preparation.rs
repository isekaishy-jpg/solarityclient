//! Referenced ground models join namespace producers one authored definition at a time.
use super::super::{RuntimeTerrainError, SharedTerrainSources};
use super::residency::{GroundDetailAssetCache, ResidentGroundDetailTile};
use solarity_asset::{
    AssetStore, BlpTextureCache, DecodedTerrainTile, GroundEffectCatalog, M2LoadDependency,
    M2ModelCache,
};
use solarity_rendering::{GroundDetailError, GroundDetailModel};
use std::{collections::BTreeSet, ops::ControlFlow, sync::Arc};

/// A tile cannot expose a partial model/texture bank while a source is pending.
pub(in super::super) struct GroundDetailPreparation {
    ids: Vec<u32>,
    next: usize,
    pending: Option<M2LoadDependency>,
    pending_texture: Option<solarity_asset::BlpLoadDependency>,
    resident: ResidentGroundDetailTile,
}
impl GroundDetailAssetCache {
    /// Capture the same sorted unique effect/model identities as synchronous terrain preparation.
    pub(in super::super) fn begin(
        &mut self,
        tile: &DecodedTerrainTile,
        store: &mut AssetStore,
    ) -> Result<GroundDetailPreparation, RuntimeTerrainError> {
        let effects = tile
            .chunks()
            .iter()
            .flat_map(|chunk| chunk.layers())
            .map(|layer| layer.effect_id())
            .filter(|id| *id != 0 && *id != u32::MAX)
            .collect::<BTreeSet<_>>();
        if effects.is_empty() {
            return Ok(GroundDetailPreparation {
                ids: Vec::new(),
                next: 0,
                pending: None,
                pending_texture: None,
                resident: ResidentGroundDetailTile::default(),
            });
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
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(GroundDetailPreparation {
            ids,
            next: 0,
            pending: None,
            pending_texture: None,
            resident: ResidentGroundDetailTile {
                catalog: Some(catalog),
                ..Default::default()
            },
        })
    }
}
impl GroundDetailPreparation {
    /// A new source decodes in this admitted turn; a shared pending source releases the worker.
    pub(in super::super) fn advance(
        &mut self,
        cache: &mut GroundDetailAssetCache,
        models: &mut M2ModelCache,
        textures: &mut BlpTextureCache,
        store: &mut AssetStore,
        shared: Option<&SharedTerrainSources>,
        suspension: &mut Option<solarity_cpu::CpuTaskDependency>,
    ) -> Result<bool, RuntimeTerrainError> {
        let Some(&id) = self.ids.get(self.next) else {
            return Ok(true);
        };
        let catalog = self
            .resident
            .catalog
            .as_ref()
            .unwrap_or_else(|| unreachable!("selected models retain their catalog"));
        let definition = catalog
            .doodad(id)
            .ok_or(GroundDetailError::MissingDoodad(id))?;
        if let std::collections::btree_map::Entry::Vacant(entry) = cache.models.entry(id) {
            let model = if let Some(shared) = shared {
                match shared.model(definition.path(), &mut self.pending, store)? {
                    ControlFlow::Break(model) => model,
                    ControlFlow::Continue(edge) => {
                        *suspension = Some(edge);
                        return Ok(false);
                    }
                }
            } else {
                models.load(store, definition.path())?
            };
            entry.insert(Arc::new(GroundDetailModel::prepare(&model)?));
        }
        let model = cache
            .models
            .get(&id)
            .ok_or(GroundDetailError::MissingDoodad(id))?;
        if !self.resident.textures.contains_key(model.texture()) {
            let source = if let Some(shared) = shared {
                match shared.texture(model.texture(), &mut self.pending_texture, store, textures)? {
                    ControlFlow::Break(source) => source,
                    ControlFlow::Continue(edge) => {
                        *suspension = Some(edge);
                        return Ok(false);
                    }
                }
            } else {
                textures.load(store, model.texture())?
            };
            self.resident
                .textures
                .insert(model.texture().clone(), source);
        }
        self.resident.models.insert(id, Arc::clone(model));
        self.next += 1;
        Ok(self.next == self.ids.len())
    }
    /// Only the completed cursor can hand immutable providers to tile publication.
    pub(in super::super) fn finish(self) -> ResidentGroundDetailTile {
        debug_assert_eq!(self.next, self.ids.len());
        self.resident
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/ground_detail_sources.rs"]
mod tests;
