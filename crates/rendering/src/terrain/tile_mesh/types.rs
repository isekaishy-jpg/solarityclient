//! Public immutable terrain tile upload and draw contracts.

use glam::Vec3;
use solarity_asset::{AssetPath, TerrainChunkIndex, TerrainTextureLayer, TerrainTileIndex};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

use crate::{TerrainRenderVertex, WorldCameraError, WorldFrustum};

/// Width and height of the 16-by-16 MCNK material atlas.
pub const TERRAIN_MATERIAL_ATLAS_WIDTH: usize = 16 * 64;

/// Byte count of the RGBA8 blend-and-shadow atlas.
pub const TERRAIN_MATERIAL_ATLAS_BYTE_COUNT: usize =
    TERRAIN_MATERIAL_ATLAS_WIDTH * TERRAIN_MATERIAL_ATLAS_WIDTH * 4;

/// Failure while aggregating a validated ADT into fixed Vulkan index ranges.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TerrainTileMeshPlanError {
    /// The combined tile no longer fits stock's compact unsigned-short index ABI.
    #[error("terrain tile geometry exceeds unsigned-short vertex indexing")]
    IndexCapacity,
    /// A combined draw range cannot be represented by Vulkan's 32-bit counters.
    #[error("terrain tile draw ranges exceed Vulkan's 32-bit counters")]
    DrawCapacity,
}

/// One MCNK draw range within an ADT-wide geometry allocation.
pub struct TerrainChunkDrawPlan {
    chunk: TerrainChunkIndex,
    first_index: u32,
    index_count: u32,
    layers: Vec<TerrainTextureLayer>,
    atlas_chunk: [u8; 2],
    bounds: [[f32; 3]; 2],
}

impl TerrainChunkDrawPlan {
    pub(super) const fn new(
        chunk: TerrainChunkIndex,
        first_index: u32,
        index_count: u32,
        layers: Vec<TerrainTextureLayer>,
        bounds: [[f32; 3]; 2],
    ) -> Self {
        Self {
            chunk,
            first_index,
            index_count,
            layers,
            atlas_chunk: [chunk.x(), chunk.y()],
            bounds,
        }
    }

    /// Returns the MCNK coordinates within the resident ADT.
    #[must_use]
    pub const fn chunk(&self) -> TerrainChunkIndex {
        self.chunk
    }

    /// Returns the first direct index in the shared tile buffer.
    #[must_use]
    pub const fn first_index(&self) -> u32 {
        self.first_index
    }

    /// Returns the number of direct indices after authored holes are removed.
    #[must_use]
    pub const fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Returns MCLY layers in exact stock blend order.
    #[must_use]
    pub fn layers(&self) -> &[TerrainTextureLayer] {
        &self.layers
    }

    /// Returns this chunk's integer location in the shared material atlas.
    #[must_use]
    pub const fn atlas_chunk(&self) -> [u8; 2] {
        self.atlas_chunk
    }

    /// Returns the world-space lower and upper culling corners.
    #[must_use]
    pub const fn bounds(&self) -> [[f32; 3]; 2] {
        self.bounds
    }

    /// Tests this draw's world AABB against an explicit camera frustum.
    ///
    /// # Errors
    ///
    /// Returns [`WorldCameraError::NonFiniteBounds`] for invalid geometry.
    pub fn is_visible(&self, frustum: WorldFrustum) -> Result<bool, WorldCameraError> {
        let [minimum, maximum] = self.bounds.map(Vec3::from_array);
        let center = (minimum + maximum) * 0.5;
        let half = (maximum - minimum) * 0.5;
        frustum.intersects_box(
            center,
            Vec3::new(half.x, 0.0, 0.0),
            Vec3::new(0.0, half.y, 0.0),
            Vec3::new(0.0, 0.0, half.z),
        )
    }
}

/// One transfer-ready ADT geometry allocation and shared material atlas.
pub struct TerrainTileMeshPlan {
    identity: u64,
    tile: TerrainTileIndex,
    vertices: Vec<TerrainRenderVertex>,
    indices: Vec<u16>,
    chunks: Vec<TerrainChunkDrawPlan>,
    textures: Vec<AssetPath>,
    texture_flags: Option<Vec<u32>>,
    material_atlas_rgba: Box<[u8; TERRAIN_MATERIAL_ATLAS_BYTE_COUNT]>,
}

impl TerrainTileMeshPlan {
    pub(super) fn new(
        tile: TerrainTileIndex,
        vertices: Vec<TerrainRenderVertex>,
        indices: Vec<u16>,
        chunks: Vec<TerrainChunkDrawPlan>,
        textures: Vec<AssetPath>,
        texture_flags: Option<Vec<u32>>,
        material_atlas_rgba: Box<[u8; TERRAIN_MATERIAL_ATLAS_BYTE_COUNT]>,
    ) -> Self {
        Self {
            identity: next_identity(),
            tile,
            vertices,
            indices,
            chunks,
            textures,
            texture_flags,
            material_atlas_rgba,
        }
    }

    pub(crate) const fn identity(&self) -> u64 {
        self.identity
    }

    /// Returns the owning ADT coordinate.
    #[must_use]
    pub const fn tile(&self) -> TerrainTileIndex {
        self.tile
    }

    /// Returns every concatenated terrain vertex.
    #[must_use]
    pub fn vertices(&self) -> &[TerrainRenderVertex] {
        &self.vertices
    }

    /// Returns compact direct indices into the combined vertex buffer.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns all 256 row-major MCNK draw ranges.
    #[must_use]
    pub fn chunks(&self) -> &[TerrainChunkDrawPlan] {
        &self.chunks
    }

    /// Returns the normalized MTEX table referenced by every chunk layer.
    #[must_use]
    pub fn textures(&self) -> &[AssetPath] {
        &self.textures
    }

    /// Returns optional raw MTXF words parallel to the resident texture table.
    #[must_use]
    pub fn texture_flags(&self) -> Option<&[u32]> {
        self.texture_flags.as_deref()
    }

    /// Returns the shared RGB blend and alpha shadow upload payload.
    #[must_use]
    pub fn material_atlas_rgba(&self) -> &[u8; TERRAIN_MATERIAL_ATLAS_BYTE_COUNT] {
        &self.material_atlas_rgba
    }

    /// Serializes vertices without relying on Rust layout or unsafe casts.
    #[must_use]
    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.vertices.len() * TerrainRenderVertex::BYTE_SIZE);
        for vertex in &self.vertices {
            vertex.append_bytes(&mut bytes);
        }
        bytes
    }

    /// Serializes the combined direct unsigned-short index buffer.
    #[must_use]
    pub fn index_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.indices.len() * size_of::<u16>());
        for index in &self.indices {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        bytes
    }
}

fn next_identity() -> u64 {
    static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);
    NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed)
}
