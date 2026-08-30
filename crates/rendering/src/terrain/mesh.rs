//! Upload-ready high-detail terrain mesh preparation for one MCNK.

use solarity_asset::{
    DecodedTerrainTile, TerrainChunk, TerrainChunkIndex, TerrainTextureLayer, TerrainTileIndex,
};

const TERRAIN_SQUARES_PER_CHUNK: usize = 8;
const TERRAIN_UNITS_PER_CHUNK: f32 = 33.333_332;
const TERRAIN_UNIT_SIZE: f32 = TERRAIN_UNITS_PER_CHUNK / TERRAIN_SQUARES_PER_CHUNK as f32;
const VERTICES_PER_CHUNK: usize = 145;
const MAXIMUM_INDICES_PER_CHUNK: usize = 8 * 8 * 4 * 3;
const NEUTRAL_VERTEX_COLOR_BGRA: [u8; 4] = [0x7F, 0x7F, 0x7F, 0xFF];

/// Fixed CPU vertex layout for the Vulkan terrain upload boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainRenderVertex {
    position: [f32; 3],
    normal: [f32; 3],
    texture_coordinates: [f32; 2],
    color_bgra: [u8; 4],
}

impl TerrainRenderVertex {
    /// Size of one explicitly serialized Vulkan terrain vertex.
    pub const BYTE_SIZE: usize = 36;

    /// Returns world position in the network/ECS coordinate convention.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns the decoded world-space normal.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        self.normal
    }

    /// Returns continuous MCNK-local terrain texture coordinates.
    #[must_use]
    pub const fn texture_coordinates(self) -> [f32; 2] {
        self.texture_coordinates
    }

    /// Returns authored MCCV data in BGRA order or stock's neutral value.
    #[must_use]
    pub const fn color_bgra(self) -> [u8; 4] {
        self.color_bgra
    }

    /// Appends the stable little-endian payload consumed by the terrain pipeline.
    fn append_bytes(self, bytes: &mut Vec<u8>) {
        for value in self.position {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in self.normal {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in self.texture_coordinates {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&self.color_bgra);
    }
}

/// Direct indexed geometry and material references for one MCNK.
pub struct TerrainChunkMeshPlan {
    tile: TerrainTileIndex,
    chunk: TerrainChunkIndex,
    vertices: Vec<TerrainRenderVertex>,
    indices: Vec<u16>,
    layers: Vec<TerrainTextureLayer>,
    alpha_bytes: Vec<u8>,
    shadow_bytes: Option<Box<[u8; 512]>>,
    bounds: [[f32; 3]; 2],
}

impl TerrainChunkMeshPlan {
    /// Prepares one exact chunk without expanding neighboring or absent data.
    #[must_use]
    pub fn prepare(tile: &DecodedTerrainTile, chunk: TerrainChunkIndex) -> Self {
        let source = &tile.chunks()
            [usize::from(chunk.y()) * TERRAIN_SQUARES_PER_CHUNK * 2 + usize::from(chunk.x())];
        let vertices = prepare_vertices(source);
        let indices = prepare_indices(source.holes());
        let bounds = vertex_bounds(&vertices);
        Self {
            tile: tile.index(),
            chunk,
            vertices,
            indices,
            layers: source.layers().to_vec(),
            alpha_bytes: source.alpha_bytes().to_vec(),
            shadow_bytes: source.shadow_bytes().map(|bytes| Box::new(*bytes)),
            bounds,
        }
    }

    /// Returns the owning ADT coordinates.
    #[must_use]
    pub const fn tile(&self) -> TerrainTileIndex {
        self.tile
    }

    /// Returns the owning MCNK coordinates.
    #[must_use]
    pub const fn chunk(&self) -> TerrainChunkIndex {
        self.chunk
    }

    /// Returns the complete 145-vertex staggered grid.
    #[must_use]
    pub fn vertices(&self) -> &[TerrainRenderVertex] {
        &self.vertices
    }

    /// Returns direct triangle indices with authored holes removed.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns MCLY layers in exact blend order.
    #[must_use]
    pub fn layers(&self) -> &[TerrainTextureLayer] {
        &self.layers
    }

    /// Returns exact MCAL bytes addressed by the layer offsets.
    #[must_use]
    pub fn alpha_bytes(&self) -> &[u8] {
        &self.alpha_bytes
    }

    /// Returns the optional packed one-bit shadow map.
    #[must_use]
    pub fn shadow_bytes(&self) -> Option<&[u8; 512]> {
        self.shadow_bytes.as_deref()
    }

    /// Returns the world-space lower and upper corners used for chunk culling.
    #[must_use]
    pub const fn bounds(&self) -> [[f32; 3]; 2] {
        self.bounds
    }

    /// Returns the number of non-hole terrain triangles.
    #[must_use]
    pub const fn triangle_count(&self) -> usize {
        self.indices.len() / 3
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

    /// Serializes direct unsigned-short indices for Vulkan upload.
    #[must_use]
    pub fn index_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.indices.len() * size_of::<u16>());
        for index in &self.indices {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        bytes
    }
}

fn vertex_bounds(vertices: &[TerrainRenderVertex]) -> [[f32; 3]; 2] {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for vertex in vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(vertex.position[axis]);
            maximum[axis] = maximum[axis].max(vertex.position[axis]);
        }
    }
    [minimum, maximum]
}

fn prepare_vertices(chunk: &TerrainChunk) -> Vec<TerrainRenderVertex> {
    let mut vertices = Vec::with_capacity(VERTICES_PER_CHUNK);
    let base = chunk.position();
    for logical_row in 0..17 {
        let inner = logical_row % 2 == 1;
        let column_count = if inner { 8 } else { 9 };
        for column in 0..column_count {
            let index = interleaved_vertex_index(logical_row, column);
            let row_units = logical_row as f32 * 0.5;
            let column_units = column as f32 + if inner { 0.5 } else { 0.0 };
            let color_bgra = chunk
                .vertex_colors_bgra()
                .map_or(NEUTRAL_VERTEX_COLOR_BGRA, |colors| colors[index]);
            vertices.push(TerrainRenderVertex {
                position: [
                    base[0] - row_units * TERRAIN_UNIT_SIZE,
                    base[1] - column_units * TERRAIN_UNIT_SIZE,
                    base[2] + chunk.heights()[index],
                ],
                normal: chunk.normals()[index],
                texture_coordinates: [
                    column_units / TERRAIN_SQUARES_PER_CHUNK as f32,
                    row_units / TERRAIN_SQUARES_PER_CHUNK as f32,
                ],
                color_bgra,
            });
        }
    }
    vertices
}

fn prepare_indices(holes: u16) -> Vec<u16> {
    let mut indices = Vec::with_capacity(MAXIMUM_INDICES_PER_CHUNK);
    for row in 0..TERRAIN_SQUARES_PER_CHUNK {
        for column in 0..TERRAIN_SQUARES_PER_CHUNK {
            if has_hole(holes, column, row) {
                continue;
            }
            let top_left = vertex_index(row * 2, column);
            let center = vertex_index(row * 2 + 1, column);
            let top_right = vertex_index(row * 2, column + 1);
            let bottom_left = vertex_index(row * 2 + 2, column);
            let bottom_right = vertex_index(row * 2 + 2, column + 1);
            indices.extend_from_slice(&[
                top_left,
                center,
                top_right,
                top_right,
                center,
                bottom_right,
                bottom_right,
                center,
                bottom_left,
                bottom_left,
                center,
                top_left,
            ]);
        }
    }
    indices
}

const fn has_hole(holes: u16, column: usize, row: usize) -> bool {
    let bit = (row / 2) * 4 + column / 2;
    holes & (1 << bit) != 0
}

fn vertex_index(logical_row: usize, column: usize) -> u16 {
    // The fixed 145-vertex MCNK domain is strictly smaller than `u16`.
    interleaved_vertex_index(logical_row, column) as u16
}

const fn interleaved_vertex_index(logical_row: usize, column: usize) -> usize {
    logical_row.div_ceil(2) * 9 + (logical_row / 2) * 8 + column
}
