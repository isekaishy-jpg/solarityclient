//! Authored material parameters and complete resident liquid texture sequences.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Weak};

use glam::Mat4;
use solarity_asset::{
    AssetError, AssetPath, AssetStore, BlpTextureCache, BlpTextureSource, LiquidMaterialCatalog,
    LiquidTypeCatalog,
};
use solarity_rendering::{
    LiquidDepthCoordinates, LiquidDepthTextureKind, LiquidTextureTimeline,
    liquid_water_surface_transform,
};
use thiserror::Error;

/// Failure to prepare the exact liquid resources named by resident geometry.
#[derive(Debug, Error)]
pub enum RuntimeLiquidAssetError {
    /// A required authored table or texture could not be read.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Native material lookup has no corresponding liquid definition.
    #[error("liquid type {id} has no definition")]
    MissingType {
        /// Missing LiquidType primary key.
        id: u32,
    },
    /// A liquid definition references an absent material row.
    #[error("liquid type {liquid} references missing material {material}")]
    MissingMaterial {
        /// Referring LiquidType primary key.
        liquid: u32,
        /// Missing LiquidMaterial primary key.
        material: u32,
    },
    /// A material requires a programmable path not implemented by this renderer.
    #[error("liquid type {liquid} requires unsupported material {material}")]
    UnsupportedMaterial {
        /// Referring LiquidType primary key.
        liquid: u32,
        /// Unsupported LiquidMaterial primary key.
        material: u32,
    },
    /// Ordinary water references an unsupported procedural texture.
    #[error("liquid type {liquid} references unsupported depth texture {texture}")]
    UnsupportedDepth {
        /// Referring LiquidType primary key.
        liquid: u32,
        /// Unrecognized procedural image name.
        texture: String,
    },
    /// An admitted material has no authored surface input.
    #[error("liquid type {liquid} has no surface texture")]
    MissingSurface {
        /// LiquidType primary key without a surface input.
        liquid: u32,
    },
    /// Combined geometry exceeds the stock unsigned-short index domain.
    #[error("liquid geometry exceeds the 16-bit vertex index domain")]
    VertexCapacity,
}

/// Parameters for the two native programmable material families.
pub(in crate::application) enum ResidentLiquidShader {
    /// Water uses separate animated surface and generated depth coordinates.
    Water {
        depth: LiquidDepthTextureKind,
        surface: Mat4,
        depth_scale: f32,
    },
    /// Magma and slime share two independent texture scrolling rates.
    Magma { rates: [f32; 2] },
}

/// A completed ordinary Texture.cpp request, including its native failure image.
pub(in crate::application) enum ResidentLiquidSurface {
    /// Selected archive texture; adjacent batches share its immutable source.
    Authored(Arc<BlpTextureSource>),
    /// 4B9760 returns AC3354's opaque green image after both load attempts fail.
    StockFailure,
}

/// One liquid definition and its complete surface sequence, shared across batches.
pub(in crate::application) struct ResidentLiquidMaterial {
    pub shader: ResidentLiquidShader,
    pub depth_coordinates: Option<LiquidDepthCoordinates>,
    pub timeline: LiquidTextureTimeline,
    pub surfaces: Vec<ResidentLiquidSurface>,
}

/// Worker-owned catalog and weak material reuse without pinning departed textures.
#[derive(Default)]
pub(in crate::application) struct LiquidAssetCache {
    catalogs: Option<(LiquidTypeCatalog, LiquidMaterialCatalog)>,
    materials: HashMap<u32, Weak<ResidentLiquidMaterial>>,
}

impl LiquidAssetCache {
    /// Loads a complete sequence once before publishing any of its geometry.
    pub(in crate::application) fn load(
        &mut self,
        id: u32,
        textures: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<Arc<ResidentLiquidMaterial>, RuntimeLiquidAssetError> {
        if let Some(material) = self.materials.get(&id).and_then(Weak::upgrade) {
            return Ok(material);
        }
        let (types, materials) = match &self.catalogs {
            Some(catalogs) => catalogs,
            None => self.catalogs.insert((
                LiquidTypeCatalog::load(store)?,
                LiquidMaterialCatalog::load(store)?,
            )),
        };
        let definition = types
            .entry(id)
            .ok_or(RuntimeLiquidAssetError::MissingType { id })?;
        let material = materials.entry(definition.material_id()).ok_or(
            RuntimeLiquidAssetError::MissingMaterial {
                liquid: id,
                material: definition.material_id(),
            },
        )?;
        let floats = definition.float_parameters();
        let integers = definition.integer_parameters();
        // 8A1FA0 dispatches by the material primary key, not its shader column.
        let (shader, period) = match (material.id(), material.is_transparent()) {
            (1, true) => {
                let depth = match definition.textures()[1].as_str() {
                    "proceduralRiverDepthTex" => LiquidDepthTextureKind::River,
                    "proceduralOceanDepthTex" => LiquidDepthTextureKind::Ocean,
                    "proceduralWmoWaterTex" => LiquidDepthTextureKind::WorldModel,
                    texture => {
                        return Err(RuntimeLiquidAssetError::UnsupportedDepth {
                            liquid: id,
                            texture: texture.to_owned(),
                        });
                    }
                };
                (
                    ResidentLiquidShader::Water {
                        depth,
                        surface: liquid_water_surface_transform(floats[0], floats[1]),
                        depth_scale: floats[2],
                    },
                    integers[1],
                )
            }
            (2, false) => (
                ResidentLiquidShader::Magma {
                    rates: [floats[0], floats[1]],
                },
                1250,
            ),
            _ => {
                return Err(RuntimeLiquidAssetError::UnsupportedMaterial {
                    liquid: id,
                    material: material.id(),
                });
            }
        };
        let surface = &definition.textures()[0];
        if surface.is_empty() {
            return Err(RuntimeLiquidAssetError::MissingSurface { liquid: id });
        }
        // 8A2450 expands precisely 1..=30. It does not scan until a missing file.
        let frame_count = if surface.contains("%d") { 30 } else { 1 };
        let mut surfaces = Vec::with_capacity(frame_count);
        for ordinal in 1..=frame_count {
            let path = AssetPath::new(surface.replace("%d", &ordinal.to_string()))?;
            surfaces.push(match textures.load(store, &path) {
                Ok(source) => ResidentLiquidSurface::Authored(source),
                Err(error) => {
                    // Stock retains the failed ordinal in its 30-entry bank.
                    // For example, owned fast_a.17.blp is absent in build 12340.
                    tracing::warn!(liquid = id, texture = %path, %error, "liquid texture request failed; using stock green texture");
                    ResidentLiquidSurface::StockFailure
                }
            });
        }
        // 79B870 admits depth coordinates only for shader zero/two and banks 0/1.
        let depth_coordinates = if matches!(material.shader(), 0 | 2) {
            match integers[0] {
                0 => Some(LiquidDepthCoordinates::River),
                1 => Some(LiquidDepthCoordinates::Ocean),
                _ => None,
            }
        } else {
            None
        };
        let resident = Arc::new(ResidentLiquidMaterial {
            shader,
            depth_coordinates,
            surfaces,
            timeline: LiquidTextureTimeline::new(
                // The sequence length is one of the two nonzero stock counts.
                NonZeroU32::new(frame_count as u32)
                    .ok_or(RuntimeLiquidAssetError::MissingSurface { liquid: id })?,
                period,
            ),
        });
        self.materials.insert(id, Arc::downgrade(&resident));
        Ok(resident)
    }
}
