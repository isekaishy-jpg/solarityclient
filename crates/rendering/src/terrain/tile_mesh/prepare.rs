//! One-pass ADT aggregation with compact indices and a shared material atlas.

use solarity_asset::{DecodedTerrainTile, TERRAIN_ALPHA_MAP_WIDTH, TERRAIN_SHADOW_MAP_WIDTH};

use crate::TerrainChunkMeshPlan;

use super::types::{
    TERRAIN_MATERIAL_ATLAS_BYTE_COUNT, TERRAIN_MATERIAL_ATLAS_WIDTH, TerrainChunkDrawPlan,
    TerrainTileMeshPlan, TerrainTileMeshPlanError,
};

impl TerrainTileMeshPlan {
    /// Aggregates all 256 MCNKs into one transfer-ready tile allocation.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainTileMeshPlanError`] if fixed stock geometry can no
    /// longer fit the compact index or Vulkan draw-counter representation.
    pub fn prepare(tile: &DecodedTerrainTile) -> Result<Self, TerrainTileMeshPlanError> {
        let mut vertices = Vec::with_capacity(tile.chunks().len() * 145);
        let mut indices = Vec::with_capacity(tile.chunks().len() * 8 * 8 * 4 * 3);
        let mut chunks = Vec::with_capacity(tile.chunks().len());
        // Build directly on the heap. `Box::new([0; N])` first materializes
        // this four-megabyte array on the comparatively small Windows stack.
        let mut atlas: Box<[u8; TERRAIN_MATERIAL_ATLAS_BYTE_COUNT]> =
            vec![0_u8; TERRAIN_MATERIAL_ATLAS_BYTE_COUNT]
                .into_boxed_slice()
                .try_into()
                .map_err(|_source| TerrainTileMeshPlanError::DrawCapacity)?;

        for source in tile.chunks() {
            let mesh = TerrainChunkMeshPlan::prepare(tile, source.index());
            let vertex_base = vertices.len();
            let first_index = u32::try_from(indices.len())
                .map_err(|_source| TerrainTileMeshPlanError::DrawCapacity)?;
            vertices.extend_from_slice(mesh.vertices());
            for index in mesh.indices() {
                indices.push(
                    u16::try_from(vertex_base + usize::from(*index))
                        .map_err(|_source| TerrainTileMeshPlanError::IndexCapacity)?,
                );
            }
            let index_count = u32::try_from(mesh.indices().len())
                .map_err(|_source| TerrainTileMeshPlanError::DrawCapacity)?;
            copy_material_map(&mesh, &mut atlas);
            chunks.push(TerrainChunkDrawPlan::new(
                mesh.chunk(),
                first_index,
                index_count,
                mesh.layers().to_vec(),
                mesh.bounds(),
            ));
        }

        Ok(Self::new(
            tile.index(),
            vertices,
            indices,
            chunks,
            tile.texture_flags().map(<[u32]>::to_vec),
            atlas,
        ))
    }
}

fn copy_material_map(
    chunk: &TerrainChunkMeshPlan,
    atlas: &mut [u8; TERRAIN_MATERIAL_ATLAS_BYTE_COUNT],
) {
    let origin_x = usize::from(chunk.chunk().x()) * TERRAIN_ALPHA_MAP_WIDTH;
    let origin_y = usize::from(chunk.chunk().y()) * TERRAIN_ALPHA_MAP_WIDTH;
    for y in 0..TERRAIN_ALPHA_MAP_WIDTH {
        for x in 0..TERRAIN_ALPHA_MAP_WIDTH {
            let source = y * TERRAIN_ALPHA_MAP_WIDTH + x;
            let destination = ((origin_y + y) * TERRAIN_MATERIAL_ATLAS_WIDTH + origin_x + x) * 4;
            if let Some(alpha) = chunk.alpha_map_rgba() {
                atlas[destination..destination + 3]
                    .copy_from_slice(&alpha[source * 4..source * 4 + 3]);
            }
            if let Some(shadow) = chunk.shadow_opacity() {
                debug_assert_eq!(TERRAIN_SHADOW_MAP_WIDTH, TERRAIN_ALPHA_MAP_WIDTH);
                atlas[destination + 3] = shadow[source];
            }
        }
    }
}
