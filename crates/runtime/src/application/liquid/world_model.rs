//! WMO group liquid factories and exact interior material selection.

use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{AssetStore, BlpTextureCache, DecodedWorldModel};
use solarity_rendering::{
    WorldModelLiquidDepthColumn, WorldModelLiquidMeshPlan, WorldModelLiquidSurface,
};

use super::{LiquidAssetCache, ResidentLiquidMaterial, RuntimeLiquidAssetError};

/// One group-local mesh and the lighting mode retained by native 7D5120.
pub(in crate::application) struct ResidentWorldModelLiquidBatch {
    pub material: Arc<ResidentLiquidMaterial>,
    pub mesh: WorldModelLiquidMeshPlan,
    pub lighting: WorldModelLiquidLighting,
    pub minimum: Vec3,
    pub maximum: Vec3,
}

/// Native interior water uses a fixed white downward light instead of world light.
#[derive(Clone, Copy)]
pub(in crate::application) enum WorldModelLiquidLighting {
    Exterior,
    Interior,
}

/// Resolves all MLIQ groups before publishing a resident root generation.
pub(in crate::application) fn prepare_world_model_liquids(
    model: &DecodedWorldModel,
    liquids: &mut LiquidAssetCache,
    textures: &mut BlpTextureCache,
    store: &mut AssetStore,
) -> Result<Vec<ResidentWorldModelLiquidBatch>, RuntimeLiquidAssetError> {
    let mut batches = Vec::new();
    for (index, group) in model.groups().iter().enumerate() {
        let Some(liquid) = group.liquid() else {
            continue;
        };
        let tint = model
            .materials()
            .get(usize::from(liquid.material_id()))
            .ok_or(RuntimeLiquidAssetError::WorldModelMaterial {
                group: index,
                material: liquid.material_id(),
            })?;
        let properties =
            liquids.world_model_properties(group.resolve_liquid_type(model.flags()), store)?;
        let mut id = properties.material_type;
        // 7BDE50 derives render-instance flag 2 from MOGI; 793D20 also
        // checks the loaded MOGP flags, then honors LiquidType flag 0x200.
        let interior = (group.flags() & 0x48 == 0 || model.group_info()[index].flags() & 0x48 == 0)
            && properties.flags & 0x200 == 0;
        if interior && (1..=20).contains(&id) && (id - 1) & 3 == 0 {
            id = 17;
        }
        let material = liquids.load(id, textures, store)?;
        let color = if interior {
            let argb = tint.diffuse_color();
            [
                (argb >> 16) as u8,
                (argb >> 8) as u8,
                argb as u8,
                (argb >> 24) as u8,
            ]
        } else {
            [255; 4]
        };
        let surface = if material.authored_surface_coordinates {
            WorldModelLiquidSurface::Magma { color }
        } else {
            WorldModelLiquidSurface::Water {
                color,
                depth: properties.depth,
                depth_column: if interior {
                    WorldModelLiquidDepthColumn::Interior
                } else {
                    WorldModelLiquidDepthColumn::Exterior
                },
            }
        };
        let Some(mesh) = WorldModelLiquidMeshPlan::prepare(model, index, surface)? else {
            continue;
        };
        if mesh.indices().is_empty() {
            continue;
        }
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for vertex in mesh.vertices() {
            let point = Vec3::from_array(vertex.position());
            minimum = minimum.min(point);
            maximum = maximum.max(point);
        }
        batches.push(ResidentWorldModelLiquidBatch {
            material,
            mesh,
            minimum,
            maximum,
            lighting: if interior {
                WorldModelLiquidLighting::Interior
            } else {
                WorldModelLiquidLighting::Exterior
            },
        });
    }
    Ok(batches)
}
