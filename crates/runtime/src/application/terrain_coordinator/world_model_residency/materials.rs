//! Authored WMO texture-stage resolution preserves stock material bindings.
use super::super::RuntimeTerrainError;
use solarity_asset::{
    AssetPath, AssetStore, BlpTextureCache, BlpTextureSource, DecodedWorldModel, WorldModelShader,
};
use std::sync::Arc;

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

/// Resolves MOMT stages in authored order before source publication.
pub(super) fn prepare_material_textures(
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

/// Keeps the stock green texture for a valid empty authored stage.
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
