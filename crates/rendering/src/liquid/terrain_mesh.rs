//! Native 7CDF80/7CE390/7CE270 terrain liquid mesh preparation.

use solarity_asset::TerrainLiquidLayer;

use super::depth::LiquidDepthCoordinates;

const GRID_STEP: f32 = -33.333_332 / 8.0;
const SURFACE_SCALE: f32 = 0.060_000_002;
const AUTHORED_UV_SCALE: f32 = 3.0 / 256.0;

/// One liquid vertex in the batch's local coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidRenderVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [u8; 4],
    depth_coordinates: [f32; 2],
    surface_coordinates: [f32; 2],
}

impl LiquidRenderVertex {
    /// Packed position, normal, RGBA color, depth UV, and surface UV extent.
    pub const BYTE_SIZE: usize = 44;

    /// Retains the complete PNC0T0T1 input shared by terrain and WMO liquids.
    #[must_use]
    pub const fn new(
        position: [f32; 3],
        normal: [f32; 3],
        color: [u8; 4],
        depth_coordinates: [f32; 2],
        surface_coordinates: [f32; 2],
    ) -> Self {
        Self {
            position,
            normal,
            color,
            depth_coordinates,
            surface_coordinates,
        }
    }

    /// Serializes the Vulkan vertex ABI without depending on Rust struct layout.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0; Self::BYTE_SIZE];
        for (index, value) in self.position.into_iter().chain(self.normal).enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[24..28].copy_from_slice(&self.color);
        for (index, value) in self
            .depth_coordinates
            .into_iter()
            .chain(self.surface_coordinates)
            .enumerate()
        {
            bytes[28 + index * 4..32 + index * 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Returns the position relative to the render instance's translation.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns the procedural depth lookup coordinates from native 79B870.
    #[must_use]
    pub const fn depth_coordinates(self) -> [f32; 2] {
        self.depth_coordinates
    }

    /// Returns the untransformed animated surface texture coordinates.
    #[must_use]
    pub const fn surface_coordinates(self) -> [f32; 2] {
        self.surface_coordinates
    }
}

/// One decoded MH2O layer, with stock strip topology including gap degenerates.
///
/// Native terrain liquid vertices have a constant upward normal and white color.
/// Positions and surface UVs include the member's translation within its batch;
/// the render instance supplies the separate batch-to-world translation.
pub struct TerrainLiquidMeshPlan {
    vertices: Box<[LiquidRenderVertex]>,
    indices: Box<[u16]>,
}

impl TerrainLiquidMeshPlan {
    /// Prepares a decoded layer using the original liquid factory inputs.
    ///
    /// `chunk_height` is MCNK base Z. `batch_translation` is the member MCNK's
    /// base minus the first member's base, as stored by 7D4AB0. `depth` is absent
    /// when 79B870 finds no supported water depth bank (including magma).
    #[must_use]
    pub fn prepare(
        layer: &TerrainLiquidLayer,
        chunk_height: f32,
        batch_translation: [f32; 3],
        depth: Option<LiquidDepthCoordinates>,
    ) -> Self {
        let width = usize::from(layer.width());
        let mut vertices = Vec::with_capacity(layer.heights().len());
        for (index, &height) in layer.heights().iter().enumerate() {
            let row = index / (width + 1) + usize::from(layer.y_offset());
            let column = index % (width + 1) + usize::from(layer.x_offset());
            // 7CDF80 stores local coordinates before 7CE390 transforms them.
            // Its adjacent global chunk coordinates cancel in x87 precision,
            // leaving this identical negative grid step throughout the map.
            let local = [
                row as f32 * GRID_STEP,
                column as f32 * GRID_STEP,
                height - chunk_height,
            ];
            let position = std::array::from_fn(|axis| local[axis] + batch_translation[axis]);
            let surface_coordinates = if layer.vertex_format() == 1 {
                // The validated decoder guarantees UV storage for format one.
                // Native explicitly tests == 1; format three uses position UVs.
                #[allow(clippy::expect_used)] // Private decoded fields enforce this invariant.
                let uv = layer
                    .texture_coordinates()
                    .expect("format one retains its decoded UV array")[index];
                uv.map(|coordinate| f32::from(coordinate) * AUTHORED_UV_SCALE)
            } else {
                [position[0] * SURFACE_SCALE, position[1] * SURFACE_SCALE]
            };
            vertices.push(LiquidRenderVertex {
                position,
                normal: [0.0, 0.0, 1.0],
                color: [255; 4],
                depth_coordinates: [
                    0.0,
                    depth.map_or(0.0, |bank| bank.coordinate(layer.depths()[index])),
                ],
                surface_coordinates,
            });
        }
        Self {
            vertices: vertices.into_boxed_slice(),
            indices: prepare_indices(layer).into_boxed_slice(),
        }
    }

    /// Returns row-major vertices, including vertices adjacent to absent cells.
    #[must_use]
    pub fn vertices(&self) -> &[LiquidRenderVertex] {
        &self.vertices
    }

    /// Returns a triangle strip with the original winding and restart degenerates.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }
}

/// 7CE270 closes each contiguous row segment with a duplicated last index.
fn prepare_indices(layer: &TerrainLiquidLayer) -> Vec<u16> {
    let width = usize::from(layer.width());
    let height = usize::from(layer.height());
    let mut indices = Vec::with_capacity(width * height * 6);
    for row in 0..height {
        let mut active = false;
        let mut last = 0;
        for column in 0..width {
            // The normalized MH2O rectangle is bounded to 8 by 8 cells.
            let top = (row * (width + 1) + column) as u16;
            let bottom = top + width as u16 + 1;
            if layer.exists()[row * width + column] != 0 {
                if !active {
                    indices.extend_from_slice(&[top, top, bottom]);
                    active = true;
                }
                last = bottom + 1;
                indices.extend_from_slice(&[top + 1, last]);
            } else if active {
                indices.push(last);
                active = false;
            }
        }
        if active {
            indices.push(last);
        }
    }
    indices
}
