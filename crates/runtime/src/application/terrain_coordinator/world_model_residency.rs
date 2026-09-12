//! Deduplicated WMO presentation residency joined to placed collision.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use glam::Vec3;
use solarity_asset::{
    AssetPath, AssetStore, BlpTextureCache, BlpTextureSource, DecodedTerrainTile,
    DecodedWorldModel, M2ModelCache, TerrainWorldModelPlacement, WmoModelCache, WorldModelShader,
};
use solarity_rendering::WorldModelMeshPlan;
use solarity_systems::{
    PlacedWorldModelCollision, PlacedWorldModelLiquid, WorldModelCollisionScene,
    WorldModelLiquidScene,
};

use crate::application::liquid::{
    LiquidAssetCache, ResidentWorldModelLiquidBatch, prepare_world_model_liquids,
};

use super::RuntimeTerrainError;
use super::m2_residency::ResidentM2SceneBuilder;

/// Worker-owned decoded generations and weak references to their prepared mesh.
/// Resident CPU/GPU sources share the plan without extending retired lifetimes.
#[derive(Default)]
pub(in crate::application) struct ResidentWorldModelCache {
    models: WmoModelCache,
    plans: HashMap<AssetPath, (Weak<DecodedWorldModel>, Weak<WorldModelMeshPlan>)>,
}

impl ResidentWorldModelCache {
    pub(in crate::application) fn new() -> Self {
        Self::default()
    }

    fn load(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<(Arc<DecodedWorldModel>, Arc<WorldModelMeshPlan>), RuntimeTerrainError> {
        let model = self.models.load(store, path)?;
        let generation = Arc::downgrade(&model);
        let retained = self.plans.get(path).and_then(|(previous, plan)| {
            previous
                .ptr_eq(&generation)
                .then(|| plan.upgrade())
                .flatten()
        });
        let plan = if let Some(plan) = retained {
            plan
        } else {
            let plan = Arc::new(WorldModelMeshPlan::prepare(&model)?);
            self.plans
                .insert(path.clone(), (generation, Arc::downgrade(&plan)));
            plan
        };
        Ok((model, plan))
    }

    pub(in crate::application) fn collect_unused(&mut self) -> usize {
        let removed = self.models.collect_unused();
        self.plans
            .retain(|_, (model, plan)| model.strong_count() > 0 && plan.strong_count() > 0);
        removed
    }
}

/// One required MapObj stage after ordinary archive resolution.
#[derive(Clone)]
pub(in crate::application) enum ResidentWorldModelTexture {
    /// A selected BLP, including an identically named HD replacement.
    Authored(Arc<BlpTextureSource>),
    /// Stock's opaque green image used for a valid empty texture slot.
    StockGreen,
}

/// The closed one/two-stage material domain retained before GPU upload.
pub(in crate::application) enum ResidentWorldModelMaterialTextures {
    /// Diffuse, specular, metal, or opaque input.
    One(ResidentWorldModelTexture),
    /// Environment, environment-metal, or unified-composite inputs.
    Two([ResidentWorldModelTexture; 2]),
}

/// One decoded root/group generation and its exact MOMT texture bindings.
pub(in crate::application) struct ResidentWorldModelSource {
    model: Arc<DecodedWorldModel>,
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
    pub(in crate::application) const fn model(&self) -> &Arc<DecodedWorldModel> {
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

/// One unique, chunk-referenced MODF instance.
pub(in crate::application) struct ResidentWorldModelPlacement {
    source_index: usize,
    unique_id: u32,
    position: Vec3,
    rotation_degrees: Vec3,
    name_set: u16,
}

impl ResidentWorldModelPlacement {
    /// Returns the MODF name set used by the WMOAreaTable tuple lookup.
    pub(in crate::application) const fn name_set(&self) -> u16 {
        self.name_set
    }
    /// Returns the MODF identity shared by references in neighboring ADTs.
    pub(in crate::application) const fn unique_id(&self) -> u32 {
        self.unique_id
    }

    /// Returns the source-table slot shared by this placement.
    pub(in crate::application) const fn source_index(&self) -> usize {
        self.source_index
    }

    /// Returns the authored WoW world position.
    pub(in crate::application) const fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns the authored MODF Euler angles in degrees.
    pub(in crate::application) const fn rotation_degrees(&self) -> Vec3 {
        self.rotation_degrees
    }
}

/// Immutable source table and unique MODF instance list for one ADT.
#[derive(Default)]
pub(in crate::application) struct ResidentWorldModelScene {
    sources: Vec<ResidentWorldModelSource>,
    placements: Vec<ResidentWorldModelPlacement>,
}

impl ResidentWorldModelScene {
    /// Returns distinct root-WMO generations in first-reference order.
    pub(in crate::application) fn sources(&self) -> &[ResidentWorldModelSource] {
        &self.sources
    }

    /// Returns unique MODF instances in first MCRF reference order.
    pub(in crate::application) fn placements(&self) -> &[ResidentWorldModelPlacement] {
        &self.placements
    }

    /// Returns the number of shared decoded root/group generations.
    pub(super) const fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Returns the number of independently transformed instances.
    pub(super) const fn placement_count(&self) -> usize {
        self.placements.len()
    }
}

/// Admits WMO presentation, collision, and liquid state as one generation.
pub(super) fn prepare_world_models(
    tile: &DecodedTerrainTile,
    model_cache: &mut ResidentWorldModelCache,
    m2_cache: &mut M2ModelCache,
    texture_cache: &mut BlpTextureCache,
    m2_builder: &mut ResidentM2SceneBuilder,
    liquid_assets: &mut LiquidAssetCache,
    store: &mut AssetStore,
) -> Result<
    (
        ResidentWorldModelScene,
        WorldModelCollisionScene,
        WorldModelLiquidScene,
    ),
    RuntimeTerrainError,
> {
    // 0x007C6150 registers MODF owners in MCRF order. The transactional
    // whole-ADT owner visits its admitted chunks in row-major order; repeated
    // references retain their first registration rather than MODF table order.
    prepare_world_model_placements(
        tile.chunks()
            .iter()
            .flat_map(|chunk| chunk.world_model_references())
            .map(|&index| &tile.world_models()[index as usize]),
        model_cache,
        m2_cache,
        texture_cache,
        m2_builder,
        liquid_assets,
        store,
    )
}

/// Admits the sole WDT-level MODF owner used by a global-WMO map.
pub(super) fn prepare_global_world_model(
    placement: &TerrainWorldModelPlacement,
    model_cache: &mut ResidentWorldModelCache,
    m2_cache: &mut M2ModelCache,
    texture_cache: &mut BlpTextureCache,
    m2_builder: &mut ResidentM2SceneBuilder,
    liquid_assets: &mut LiquidAssetCache,
    store: &mut AssetStore,
) -> Result<
    (
        ResidentWorldModelScene,
        WorldModelCollisionScene,
        WorldModelLiquidScene,
    ),
    RuntimeTerrainError,
> {
    prepare_world_model_placements(
        std::iter::once(placement),
        model_cache,
        m2_cache,
        texture_cache,
        m2_builder,
        liquid_assets,
        store,
    )
}

/// Joins selected MODF owners to their shared WMO, collision, and MODD state.
fn prepare_world_model_placements<'placement>(
    placements: impl IntoIterator<Item = &'placement TerrainWorldModelPlacement>,
    model_cache: &mut ResidentWorldModelCache,
    m2_cache: &mut M2ModelCache,
    texture_cache: &mut BlpTextureCache,
    m2_builder: &mut ResidentM2SceneBuilder,
    liquid_assets: &mut LiquidAssetCache,
    store: &mut AssetStore,
) -> Result<
    (
        ResidentWorldModelScene,
        WorldModelCollisionScene,
        WorldModelLiquidScene,
    ),
    RuntimeTerrainError,
> {
    let mut result = ResidentWorldModelScene::default();
    let mut collision = WorldModelCollisionScene::new();
    let mut liquids = WorldModelLiquidScene::new();
    let mut source_indices = HashMap::<AssetPath, usize>::new();
    let mut placement_owners = HashMap::<u32, &TerrainWorldModelPlacement>::new();
    for placement in placements {
        if let Some(previous) = placement_owners.insert(placement.unique_id(), placement) {
            if !same_world_model_placement(previous, placement) {
                return Err(RuntimeTerrainError::ConflictingWorldModelPlacement {
                    unique_id: placement.unique_id(),
                });
            }
            continue;
        }

        let source_index = if let Some(source_index) = source_indices.get(placement.path()) {
            *source_index
        } else {
            let source_index = result.sources.len();
            result.sources.push(ResidentWorldModelSource::load(
                placement.path(),
                model_cache,
                texture_cache,
                liquid_assets,
                store,
            )?);
            source_indices.insert(placement.path().clone(), source_index);
            source_index
        };
        let source = &result.sources[source_index];
        let position = Vec3::from_array(placement.position());
        let rotation_degrees = Vec3::from_array(placement.rotation());
        collision.add(PlacedWorldModelCollision::prepare(
            Arc::clone(source.model()),
            position,
            rotation_degrees,
            1.0,
        )?);
        liquids.add(PlacedWorldModelLiquid::prepare(
            Arc::clone(source.model()),
            position,
            rotation_degrees,
            1.0,
        )?);
        result.placements.push(ResidentWorldModelPlacement {
            source_index,
            unique_id: placement.unique_id(),
            position,
            rotation_degrees,
            name_set: placement.name_set(),
        });
        for doodad_index in source
            .model()
            .referenced_active_doodad_indices(placement.doodad_set())?
        {
            // The active-index resolver and root admission jointly prove that
            // this table lookup is in range.
            let doodad = &source.model().doodads()[doodad_index];
            m2_builder.add_world_model_doodad(
                placement,
                doodad_index,
                doodad,
                m2_cache,
                texture_cache,
                store,
            )?;
        }
    }
    Ok((result, collision, liquids))
}

fn prepare_material_textures(
    model: &DecodedWorldModel,
    texture_cache: &mut BlpTextureCache,
    store: &mut AssetStore,
) -> Result<Vec<ResidentWorldModelMaterialTextures>, RuntimeTerrainError> {
    model
        .materials()
        .iter()
        .map(|material| {
            let first = prepare_texture(material.textures()[0].as_ref(), texture_cache, store)?;
            Ok(match material.shader() {
                WorldModelShader::Environment
                | WorldModelShader::EnvironmentMetal
                | WorldModelShader::Composite => {
                    let second =
                        prepare_texture(material.textures()[1].as_ref(), texture_cache, store)?;
                    ResidentWorldModelMaterialTextures::Two([first, second])
                }
                WorldModelShader::Diffuse
                | WorldModelShader::Specular
                | WorldModelShader::Metal
                | WorldModelShader::Opaque => ResidentWorldModelMaterialTextures::One(first),
            })
        })
        .collect()
}

fn prepare_texture(
    path: Option<&AssetPath>,
    cache: &mut BlpTextureCache,
    store: &mut AssetStore,
) -> Result<ResidentWorldModelTexture, RuntimeTerrainError> {
    path.map_or(Ok(ResidentWorldModelTexture::StockGreen), |path| {
        cache
            .load(store, path)
            .map(ResidentWorldModelTexture::Authored)
            .map_err(RuntimeTerrainError::from)
    })
}

pub(super) fn same_world_model_placement(
    left: &TerrainWorldModelPlacement,
    right: &TerrainWorldModelPlacement,
) -> bool {
    left.path() == right.path()
        && left.position().map(f32::to_bits) == right.position().map(f32::to_bits)
        && left.rotation().map(f32::to_bits) == right.rotation().map(f32::to_bits)
        && left.bounds().map(|point| point.map(f32::to_bits))
            == right.bounds().map(|point| point.map(f32::to_bits))
        && left.flags() == right.flags()
        && left.doodad_set() == right.doodad_set()
        && left.name_set() == right.name_set()
}
