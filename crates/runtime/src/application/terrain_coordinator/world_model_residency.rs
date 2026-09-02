//! Deduplicated WMO presentation residency joined to placed collision.

use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{
    AssetPath, AssetStore, BlpTextureCache, BlpTextureSource, DecodedTerrainTile,
    DecodedWorldModel, M2ModelCache, TerrainWorldModelPlacement, WmoModelCache, WorldModelShader,
};
use solarity_systems::{
    PlacedWorldModelCollision, PlacedWorldModelLiquid, WorldModelCollisionScene,
    WorldModelLiquidScene,
};

use super::RuntimeTerrainError;
use super::m2_residency::ResidentM2SceneBuilder;

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
    materials: Vec<ResidentWorldModelMaterialTextures>,
}

impl ResidentWorldModelSource {
    /// Loads one complete root/group generation and every MOMT texture stage.
    pub(in crate::application) fn load(
        path: &AssetPath,
        model_cache: &mut WmoModelCache,
        texture_cache: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<Self, RuntimeTerrainError> {
        let model = model_cache.load(store, path)?;
        let materials = prepare_material_textures(&model, texture_cache, store)?;
        Ok(Self { model, materials })
    }

    /// Returns the immutable root/group generation selected by MPQ priority.
    pub(in crate::application) const fn model(&self) -> &Arc<DecodedWorldModel> {
        &self.model
    }

    /// Returns material stages in exact MOMT table order.
    pub(in crate::application) fn materials(&self) -> &[ResidentWorldModelMaterialTextures] {
        &self.materials
    }
}

/// One unique, chunk-referenced MODF instance.
pub(in crate::application) struct ResidentWorldModelPlacement {
    source_index: usize,
    position: Vec3,
    rotation_degrees: Vec3,
}

impl ResidentWorldModelPlacement {
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

    /// Returns unique referenced MODF instances in file order.
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
    model_cache: &mut WmoModelCache,
    m2_cache: &mut M2ModelCache,
    texture_cache: &mut BlpTextureCache,
    m2_builder: &mut ResidentM2SceneBuilder,
    store: &mut AssetStore,
) -> Result<
    (
        ResidentWorldModelScene,
        WorldModelCollisionScene,
        WorldModelLiquidScene,
    ),
    RuntimeTerrainError,
> {
    let mut referenced = vec![false; tile.world_models().len()];
    for reference in tile
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.world_model_references())
    {
        // Strict ADT decoding has already proven every MCRF index is in range.
        referenced[*reference as usize] = true;
    }

    let mut result = ResidentWorldModelScene::default();
    let mut collision = WorldModelCollisionScene::new();
    let mut liquids = WorldModelLiquidScene::new();
    let mut source_indices = HashMap::<AssetPath, usize>::new();
    let mut placement_indices = HashMap::<u32, usize>::new();
    for (index, placement) in tile.world_models().iter().enumerate() {
        if !referenced[index] {
            continue;
        }
        if let Some(previous_index) = placement_indices.insert(placement.unique_id(), index) {
            if !same_world_model_placement(&tile.world_models()[previous_index], placement) {
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
            position,
            rotation_degrees,
        });
        for doodad_index in source
            .model()
            .active_doodad_indices(placement.doodad_set())?
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

fn same_world_model_placement(
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
