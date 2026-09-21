//! Complete WMO source products retain geometry, material and liquid inputs.
use super::super::RuntimeTerrainError;
use super::{
    ResidentWorldModelCache, ResidentWorldModelMaterialTextures,
    materials::prepare_material_textures,
};
use crate::application::liquid::{
    LiquidAssetCache, ResidentWorldModelLiquidBatch, prepare_world_model_liquids,
};
use solarity_asset::{AssetPath, AssetStore, BlpTextureCache, DecodedWorldModel, ResourceLease};
use solarity_rendering::WorldModelMeshPlan;
use std::sync::Arc;

/// One decoded root/group generation and its exact MOMT texture bindings.
pub(in crate::application) struct ResidentWorldModelSource {
    model: ResourceLease<DecodedWorldModel>,
    plan: Arc<WorldModelMeshPlan>,
    materials: Vec<ResidentWorldModelMaterialTextures>,
    liquids: Vec<ResidentWorldModelLiquidBatch>,
}

impl ResidentWorldModelSource {
    /// Loads one complete root/group generation and every MOMT texture stage.
    pub(in crate::application) fn load(
        path: &AssetPath,
        model_cache: &mut ResidentWorldModelCache,
        texture_cache: &mut BlpTextureCache,
        liquid_assets: &mut LiquidAssetCache,
        store: &mut AssetStore,
    ) -> Result<Self, RuntimeTerrainError> {
        let (model, plan) = model_cache.load(store, path)?;
        Self::from_prepared(model, plan, texture_cache, liquid_assets, store)
    }

    /// Consumes the exact shared root/group result before preparing its materials and liquids.
    pub(in crate::application) fn from_model(
        model: ResourceLease<DecodedWorldModel>,
        model_cache: &mut ResidentWorldModelCache,
        texture_cache: &mut BlpTextureCache,
        liquid_assets: &mut LiquidAssetCache,
        store: &mut AssetStore,
    ) -> Result<Self, RuntimeTerrainError> {
        let (model, plan) = model_cache.prepare_model(model)?;
        Self::from_prepared(model, plan, texture_cache, liquid_assets, store)
    }

    /// Texture/liquid order remains identical for synchronous and dependency-driven loading.
    fn from_prepared(
        model: ResourceLease<DecodedWorldModel>,
        plan: Arc<WorldModelMeshPlan>,
        texture_cache: &mut BlpTextureCache,
        liquid_assets: &mut LiquidAssetCache,
        store: &mut AssetStore,
    ) -> Result<Self, RuntimeTerrainError> {
        let materials = prepare_material_textures(&model, texture_cache, store)?;
        let liquids = prepare_world_model_liquids(&model, liquid_assets, texture_cache, store)?;
        Ok(Self {
            model,
            plan,
            materials,
            liquids,
        })
    }

    /// Returns the complete worker-prepared surface and shadow geometry.
    pub(in crate::application) const fn plan(&self) -> &Arc<WorldModelMeshPlan> {
        &self.plan
    }

    /// Returns the immutable root/group generation selected by MPQ priority.
    pub(in crate::application) const fn model(&self) -> &ResourceLease<DecodedWorldModel> {
        &self.model
    }

    /// Returns the complete group-local liquid factories for this root.
    pub(in crate::application) fn liquids(&self) -> &[ResidentWorldModelLiquidBatch] {
        &self.liquids
    }

    /// Returns material stages in exact MOMT table order.
    pub(in crate::application) fn materials(&self) -> &[ResidentWorldModelMaterialTextures] {
        &self.materials
    }
}
