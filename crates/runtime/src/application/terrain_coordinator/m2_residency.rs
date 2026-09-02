//! Shared placed-M2 residency for ADT MDDF and WMO MODD owners.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use solarity_asset::{
    AssetPath, AssetStore, BlpTextureCache, BlpTextureSource, DecodedM2Model, M2ModelCache,
    M2TextureKind, TerrainDoodadPlacement, TerrainWorldModelPlacement, WorldModelDoodad,
};
use solarity_systems::{M2CollisionScene, PlacedM2Collision};

use super::RuntimeTerrainError;

/// The authored owner of one independently transformed M2 instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) enum ResidentM2Owner {
    /// A terrain MDDF record selected through an MCNK MCRF reference.
    TerrainDoodad { unique_id: u32 },
    /// A WMO MODD record selected through its owning MODF doodad set.
    WorldModelDoodad {
        world_model_unique_id: u32,
        doodad_index: usize,
    },
}

/// One model texture declaration after archive-backed hardcoded resolution.
pub(in crate::application) enum ResidentM2Texture {
    /// A concrete BLP selected through ordinary MPQ precedence.
    Authored(Arc<BlpTextureSource>),
    /// A display/customization input which this static world owner cannot fill.
    Replaceable(M2TextureKind),
}

/// One archive-selected M2 generation shared by every resident owner.
pub(in crate::application) struct ResidentM2Source {
    model: Arc<DecodedM2Model>,
    textures: Vec<ResidentM2Texture>,
}

impl ResidentM2Source {
    /// Loads one complete M2/SKIN generation and its authored texture inputs.
    pub(in crate::application) fn load(
        path: &AssetPath,
        cache: &mut M2ModelCache,
        texture_cache: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<Self, RuntimeTerrainError> {
        let model = cache.load(store, path)?;
        let textures = prepare_textures(&model, texture_cache, store)?;
        Ok(Self { model, textures })
    }

    /// Returns the immutable M2/SKIN generation selected by MPQ precedence.
    pub(in crate::application) const fn model(&self) -> &Arc<DecodedM2Model> {
        &self.model
    }

    /// Returns one resident source for every model texture declaration.
    pub(in crate::application) fn textures(&self) -> &[ResidentM2Texture] {
        &self.textures
    }
}

/// One placed M2 with its already composed local-to-world transform.
pub(in crate::application) struct ResidentM2Placement {
    source_index: usize,
    transform: Mat4,
    owner: ResidentM2Owner,
    flags: u16,
    color: [u8; 4],
}

impl ResidentM2Placement {
    /// Returns the shared M2 generation used by this independent placement.
    pub(in crate::application) const fn source_index(&self) -> usize {
        self.source_index
    }

    /// Returns the fully composed model-local to world-space transform.
    pub(in crate::application) const fn transform(&self) -> Mat4 {
        self.transform
    }

    /// Returns the stock placement table which authored this instance.
    pub(in crate::application) const fn owner(&self) -> ResidentM2Owner {
        self.owner
    }

    /// Returns the unmodified MDDF or MODD flags.
    pub(in crate::application) const fn flags(&self) -> u16 {
        self.flags
    }

    /// Returns the packed MODD color, or opaque white for an MDDF placement.
    pub(in crate::application) const fn color(&self) -> [u8; 4] {
        self.color
    }
}

/// Deduplicated M2 generations and all independently transformed owners.
#[derive(Default)]
pub(in crate::application) struct ResidentM2Scene {
    sources: Vec<ResidentM2Source>,
    placements: Vec<ResidentM2Placement>,
}

impl ResidentM2Scene {
    /// Returns every archive-selected model generation in stable slot order.
    pub(in crate::application) fn sources(&self) -> &[ResidentM2Source] {
        &self.sources
    }

    /// Returns all independently transformed MDDF and MODD instances.
    pub(in crate::application) fn placements(&self) -> &[ResidentM2Placement] {
        &self.placements
    }

    pub(super) const fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub(super) const fn placement_count(&self) -> usize {
        self.placements.len()
    }

    pub(super) fn authored_texture_count(&self) -> usize {
        self.sources
            .iter()
            .flat_map(ResidentM2Source::textures)
            .filter(|texture| matches!(texture, ResidentM2Texture::Authored(_)))
            .count()
    }

    pub(super) fn replaceable_texture_count(&self) -> usize {
        self.sources
            .iter()
            .flat_map(ResidentM2Source::textures)
            .filter(|texture| matches!(texture, ResidentM2Texture::Replaceable(_)))
            .count()
    }
}

/// Transactional builder sharing source identities across MDDF and MODD.
pub(super) struct ResidentM2SceneBuilder {
    scene: ResidentM2Scene,
    collision: M2CollisionScene,
    source_indices: HashMap<AssetPath, usize>,
}

impl ResidentM2SceneBuilder {
    pub(super) fn new() -> Self {
        Self {
            scene: ResidentM2Scene::default(),
            collision: M2CollisionScene::new(),
            source_indices: HashMap::new(),
        }
    }

    pub(super) fn add_terrain_doodad(
        &mut self,
        placement: &TerrainDoodadPlacement,
        cache: &mut M2ModelCache,
        texture_cache: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<(), RuntimeTerrainError> {
        let transform = adt_placement_transform(
            Vec3::from_array(placement.position()),
            Vec3::from_array(placement.rotation()),
            placement.scale(),
        )?;
        self.add(
            placement.path(),
            transform,
            ResidentM2Owner::TerrainDoodad {
                unique_id: placement.unique_id(),
            },
            placement.flags(),
            [u8::MAX; 4],
            cache,
            texture_cache,
            store,
        )
    }

    pub(super) fn add_world_model_doodad(
        &mut self,
        owner: &TerrainWorldModelPlacement,
        doodad_index: usize,
        doodad: &WorldModelDoodad,
        cache: &mut M2ModelCache,
        texture_cache: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<(), RuntimeTerrainError> {
        let outer = adt_placement_transform(
            Vec3::from_array(owner.position()),
            Vec3::from_array(owner.rotation()),
            1.0,
        )?;
        let local = world_model_doodad_transform(doodad)?;
        self.add(
            doodad.path(),
            outer * local,
            ResidentM2Owner::WorldModelDoodad {
                world_model_unique_id: owner.unique_id(),
                doodad_index,
            },
            u16::from(doodad.flags()),
            doodad.color(),
            cache,
            texture_cache,
            store,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        path: &AssetPath,
        transform: Mat4,
        owner: ResidentM2Owner,
        flags: u16,
        color: [u8; 4],
        cache: &mut M2ModelCache,
        texture_cache: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<(), RuntimeTerrainError> {
        // Cache loading performs stock's MDL/MDX-to-M2 conversion. Key the
        // scene by that returned canonical identity so an ADT `.m2` and WMO
        // MODN `.mdx` reference still share one generation.
        let model = cache.load(store, path)?;
        let source_index = if let Some(index) = self.source_indices.get(model.path()) {
            *index
        } else {
            let index = self.scene.sources.len();
            self.scene
                .sources
                .push(ResidentM2Source::load(path, cache, texture_cache, store)?);
            self.source_indices.insert(model.path().clone(), index);
            index
        };
        let model = Arc::clone(self.scene.sources[source_index].model());
        self.collision
            .add(PlacedM2Collision::prepare_transform(model, transform)?);
        self.scene.placements.push(ResidentM2Placement {
            source_index,
            transform,
            owner,
            flags,
            color,
        });
        Ok(())
    }

    pub(super) fn finish(self) -> (ResidentM2Scene, M2CollisionScene) {
        // Admission constructs these records transactionally. Retain a debug
        // proof over every field so future renderer publication cannot inherit
        // a stale source index or a partially composed owner record.
        for placement in &self.scene.placements {
            debug_assert!(placement.source_index < self.scene.sources.len());
            debug_assert!(
                placement.transform.is_finite()
                    && placement.transform.determinant().abs() > f32::EPSILON
            );
            tracing::trace!(
                owner = ?placement.owner,
                source_index = placement.source_index,
                flags = placement.flags,
                color = ?placement.color,
                "placed M2 entered resident tile scene"
            );
        }
        (self.scene, self.collision)
    }
}

/// Resolves hardcoded BLPs while preserving replacement categories as holes.
fn prepare_textures(
    model: &DecodedM2Model,
    cache: &mut BlpTextureCache,
    store: &mut AssetStore,
) -> Result<Vec<ResidentM2Texture>, RuntimeTerrainError> {
    model
        .textures()
        .iter()
        .map(|texture| {
            if texture.kind() != M2TextureKind::Hardcoded {
                return Ok(ResidentM2Texture::Replaceable(texture.kind()));
            }
            let path = texture.filename().ok_or_else(|| {
                RuntimeTerrainError::MissingM2HardcodedTexturePath {
                    model: model.path().clone(),
                }
            })?;
            cache
                .load(store, path)
                .map(ResidentM2Texture::Authored)
                .map_err(RuntimeTerrainError::from)
        })
        .collect()
}

/// Reproduces the build-12340 MDDF/MODF Euler placement matrix.
fn adt_placement_transform(
    position: Vec3,
    rotation_degrees: Vec3,
    scale: f32,
) -> Result<Mat4, RuntimeTerrainError> {
    if !position.is_finite() || !rotation_degrees.is_finite() || !scale.is_finite() || scale <= 0.0
    {
        return Err(RuntimeTerrainError::InvalidM2Placement);
    }
    let radians = Vec3::new(
        rotation_degrees.x.to_radians(),
        rotation_degrees.y.to_radians(),
        rotation_degrees.z.to_radians(),
    );
    validate_transform(
        Mat4::from_translation(position)
            * Mat4::from_rotation_z(radians.y + std::f32::consts::PI)
            * Mat4::from_rotation_y(radians.x)
            * Mat4::from_rotation_x(radians.z)
            * Mat4::from_scale(Vec3::splat(scale)),
    )
}

/// Builds the root-local MODD matrix from its on-disk XYZW quaternion.
fn world_model_doodad_transform(doodad: &WorldModelDoodad) -> Result<Mat4, RuntimeTerrainError> {
    let orientation = Quat::from_array(doodad.orientation());
    validate_transform(
        Mat4::from_translation(Vec3::from_array(doodad.position()))
            * Mat4::from_quat(orientation)
            * Mat4::from_scale(Vec3::splat(doodad.scale())),
    )
}

fn validate_transform(transform: Mat4) -> Result<Mat4, RuntimeTerrainError> {
    if !transform.is_finite() || transform.determinant().abs() <= f32::EPSILON {
        return Err(RuntimeTerrainError::InvalidM2Placement);
    }
    Ok(transform)
}
