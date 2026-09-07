//! Stock programmable-terrain liquid batches built before worker publication.

use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{AssetStore, BlpTextureCache, DecodedTerrainTile, TerrainLiquidLayer};
use solarity_rendering::{LiquidRenderVertex, TerrainLiquidMeshPlan};

use super::{LiquidAssetCache, ResidentLiquidMaterial, RuntimeLiquidAssetError};

/// One native 2-by-2 MCNK material batch and its first member's world origin.
pub(in crate::application) struct ResidentTerrainLiquidBatch {
    pub material: Arc<ResidentLiquidMaterial>,
    pub origin: Vec3,
    pub vertices: Vec<LiquidRenderVertex>,
    pub indices: Vec<u16>,
    pub minimum: Vec3,
    pub maximum: Vec3,
}

/// Groups same-type members in the row-major order used by native 7CF200.
///
/// 780F50 enables both shader flags on programmable hardware. Successful terrain
/// shaders make 7BD8A0 select the 2-by-2 batch path consumed by this Vulkan client.
pub(in crate::application) fn prepare_terrain_liquids(
    tile: &DecodedTerrainTile,
    liquids: &mut LiquidAssetCache,
    textures: &mut BlpTextureCache,
    store: &mut AssetStore,
) -> Result<Vec<ResidentTerrainLiquidBatch>, RuntimeLiquidAssetError> {
    let Some(table) = tile.liquids() else {
        return Ok(Vec::new());
    };
    let mut batches = Vec::new();
    for row in (0..16).step_by(2) {
        for column in (0..16).step_by(2) {
            let mut members: Vec<(u16, Vec<(Vec3, &TerrainLiquidLayer)>)> = Vec::new();
            for chunk_index in [
                row * 16 + column,
                row * 16 + column + 1,
                (row + 1) * 16 + column,
                (row + 1) * 16 + column + 1,
            ] {
                for layer in table.chunks()[chunk_index].layers() {
                    let id = layer.liquid_type();
                    let entry = if let Some(index) = members.iter().position(|(key, _)| *key == id)
                    {
                        &mut members[index].1
                    } else {
                        members.push((id, Vec::new()));
                        &mut members
                            .last_mut()
                            .ok_or(RuntimeLiquidAssetError::VertexCapacity)?
                            .1
                    };
                    entry.push((
                        Vec3::from_array(tile.chunks()[chunk_index].position()),
                        layer,
                    ));
                }
            }
            for (id, members) in members {
                let material = liquids.load(u32::from(id), textures, store)?;
                let Some((origin, _)) = members.first().copied() else {
                    continue;
                };
                let mut batch = ResidentTerrainLiquidBatch {
                    material,
                    origin,
                    vertices: Vec::new(),
                    indices: Vec::new(),
                    minimum: Vec3::splat(f32::INFINITY),
                    maximum: Vec3::splat(f32::NEG_INFINITY),
                };
                for (position, layer) in members {
                    let plan = TerrainLiquidMeshPlan::prepare(
                        layer,
                        position.z,
                        (position - origin).to_array(),
                        batch.material.depth_coordinates,
                    );
                    let base = u16::try_from(batch.vertices.len())
                        .map_err(|_| RuntimeLiquidAssetError::VertexCapacity)?;
                    for &index in plan.indices() {
                        batch.indices.push(
                            base.checked_add(index)
                                .ok_or(RuntimeLiquidAssetError::VertexCapacity)?,
                        );
                    }
                    for &vertex in plan.vertices() {
                        let world = Vec3::from_array(vertex.position()) + origin;
                        batch.minimum = batch.minimum.min(world);
                        batch.maximum = batch.maximum.max(world);
                        batch.vertices.push(vertex);
                    }
                }
                if !batch.indices.is_empty() {
                    batches.push(batch);
                }
            }
        }
    }
    Ok(batches)
}
