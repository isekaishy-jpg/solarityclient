//! Point sampling over normalized ADT MH2O liquid surfaces.

use glam::{Vec2, Vec3};
use solarity_asset::{DecodedTerrainTile, TerrainLiquidLayer};
use thiserror::Error;

const TERRAIN_TILE_SIZE: f32 = 533.333_3;
const LIQUID_GRID_STEPS_PER_TILE: f32 = 128.0;
const SAMPLE_TOLERANCE: f32 = 0.0001;

/// Stock liquid information at one admitted world-space point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainLiquidSample {
    height: f32,
    liquid_type: u16,
    fishable: bool,
    deep: bool,
}

impl TerrainLiquidSample {
    /// Returns the interpolated absolute surface height.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }

    /// Returns the `LiquidType.dbc` identifier.
    #[must_use]
    pub const fn liquid_type(self) -> u16 {
        self.liquid_type
    }

    /// Returns whether the sampled 8-by-8 MCNK cell is fishable.
    #[must_use]
    pub const fn is_fishable(self) -> bool {
        self.fishable
    }

    /// Returns whether the sampled 8-by-8 MCNK cell is deep.
    #[must_use]
    pub const fn is_deep(self) -> bool {
        self.deep
    }
}

/// Invalid normalized liquid geometry or point-query input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TerrainLiquidError {
    /// A normalized layer's component arrays disagree with its rectangle.
    #[error("terrain liquid arrays do not match their authored dimensions")]
    InvalidArrays,
    /// A generated liquid vertex is NaN or infinite.
    #[error("terrain liquid geometry is not finite")]
    NonFiniteGeometry,
    /// The query's world X or Y coordinate is NaN or infinite.
    #[error("terrain liquid query point is not finite")]
    NonFinitePoint,
    /// The optional reference height is NaN or infinite.
    #[error("terrain liquid reference height is not finite")]
    NonFiniteReferenceHeight,
}

/// Immutable point-query geometry for one decoded ADT generation.
pub struct TerrainLiquidMesh {
    draws: Vec<TerrainLiquidDraw>,
}

impl TerrainLiquidMesh {
    /// Builds stock liquid triangles for every existing MH2O cell.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainLiquidError`] if normalized arrays disagree or a
    /// decoded height would produce non-finite world geometry.
    pub fn prepare(tile: &DecodedTerrainTile) -> Result<Self, TerrainLiquidError> {
        let Some(table) = tile.liquids() else {
            return Ok(Self { draws: Vec::new() });
        };
        let mut draws = Vec::with_capacity(table.layer_count());
        for (chunk_index, chunk) in table.chunks().iter().enumerate() {
            let chunk_x = chunk_index % 16;
            let chunk_y = chunk_index / 16;
            for layer in chunk.layers() {
                if let Some(draw) = TerrainLiquidDraw::prepare(
                    tile.index().x(),
                    tile.index().y(),
                    chunk_x,
                    chunk_y,
                    chunk.fishable_mask(),
                    chunk.deep_mask(),
                    layer,
                )? {
                    draws.push(draw);
                }
            }
        }
        Ok(Self { draws })
    }

    /// Samples the preferred authored liquid surface at one world X/Y point.
    ///
    /// Without a reference height, the highest stacked surface wins. With a
    /// reference, stock first prefers surfaces at or above it, choosing the
    /// nearest such surface; otherwise it chooses the nearest surface below.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainLiquidError`] for non-finite point or reference input.
    pub fn sample(
        &self,
        world_x: f32,
        world_y: f32,
        reference_height: Option<f32>,
    ) -> Result<Option<TerrainLiquidSample>, TerrainLiquidError> {
        if !world_x.is_finite() || !world_y.is_finite() {
            return Err(TerrainLiquidError::NonFinitePoint);
        }
        if reference_height.is_some_and(|height| !height.is_finite()) {
            return Err(TerrainLiquidError::NonFiniteReferenceHeight);
        }
        let point = Vec2::new(world_x, world_y);
        let mut selected: Option<TerrainLiquidSample> = None;
        for draw in &self.draws {
            if point.x < draw.minimum.x - SAMPLE_TOLERANCE
                || point.x > draw.maximum.x + SAMPLE_TOLERANCE
                || point.y < draw.minimum.y - SAMPLE_TOLERANCE
                || point.y > draw.maximum.y + SAMPLE_TOLERANCE
            {
                continue;
            }
            for triangle in &draw.triangles {
                let Some(height) = sample_triangle_height(triangle.vertices, point) else {
                    continue;
                };
                if selected
                    .is_some_and(|current| !prefer_height(height, current.height, reference_height))
                {
                    continue;
                }
                selected = Some(TerrainLiquidSample {
                    height,
                    liquid_type: draw.liquid_type,
                    fishable: draw.fishable_mask & (1_u64 << triangle.cell_bit) != 0,
                    deep: draw.deep_mask & (1_u64 << triangle.cell_bit) != 0,
                });
            }
        }
        Ok(selected)
    }
}

struct TerrainLiquidDraw {
    liquid_type: u16,
    fishable_mask: u64,
    deep_mask: u64,
    minimum: Vec2,
    maximum: Vec2,
    triangles: Vec<TerrainLiquidTriangle>,
}

impl TerrainLiquidDraw {
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        tile_x: u8,
        tile_y: u8,
        chunk_x: usize,
        chunk_y: usize,
        fishable_mask: u64,
        deep_mask: u64,
        layer: &TerrainLiquidLayer,
    ) -> Result<Option<Self>, TerrainLiquidError> {
        let row_stride = usize::from(layer.width()) + 1;
        let vertex_count = row_stride * (usize::from(layer.height()) + 1);
        let cell_count = usize::from(layer.width()) * usize::from(layer.height());
        if layer.heights().len() != vertex_count
            || layer.depths().len() != vertex_count
            || layer.exists().len() != cell_count
            || layer
                .texture_coordinates()
                .is_some_and(|coordinates| coordinates.len() != vertex_count)
        {
            return Err(TerrainLiquidError::InvalidArrays);
        }
        let mut vertices = Vec::with_capacity(vertex_count);
        for row in 0..=usize::from(layer.height()) {
            for column in 0..=usize::from(layer.width()) {
                let index = row * row_stride + column;
                let across_world_x = (chunk_y * 8 + usize::from(layer.y_offset()) + row) as f32;
                let across_world_y = (chunk_x * 8 + usize::from(layer.x_offset()) + column) as f32;
                let position = Vec3::new(
                    (32.0 - f32::from(tile_y) - across_world_x / LIQUID_GRID_STEPS_PER_TILE)
                        * TERRAIN_TILE_SIZE,
                    (32.0 - f32::from(tile_x) - across_world_y / LIQUID_GRID_STEPS_PER_TILE)
                        * TERRAIN_TILE_SIZE,
                    layer.heights()[index],
                );
                if !position.is_finite() {
                    return Err(TerrainLiquidError::NonFiniteGeometry);
                }
                vertices.push(position);
            }
        }

        let (minimum, maximum) = vertices.iter().fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), vertex| {
                let point = vertex.truncate();
                (minimum.min(point), maximum.max(point))
            },
        );
        let mut triangles = Vec::with_capacity(cell_count * 2);
        for row in 0..usize::from(layer.height()) {
            for column in 0..usize::from(layer.width()) {
                let cell = row * usize::from(layer.width()) + column;
                if layer.exists()[cell] == 0 {
                    continue;
                }
                let top_left = row * row_stride + column;
                let top_right = top_left + 1;
                let bottom_left = top_left + row_stride;
                let bottom_right = bottom_left + 1;
                let cell_bit = ((usize::from(layer.y_offset()) + row) * 8
                    + usize::from(layer.x_offset())
                    + column) as u8;
                triangles.push(TerrainLiquidTriangle {
                    vertices: [
                        vertices[top_left],
                        vertices[bottom_left],
                        vertices[top_right],
                    ],
                    cell_bit,
                });
                triangles.push(TerrainLiquidTriangle {
                    vertices: [
                        vertices[top_right],
                        vertices[bottom_left],
                        vertices[bottom_right],
                    ],
                    cell_bit,
                });
            }
        }
        if triangles.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            liquid_type: layer.liquid_type(),
            fishable_mask,
            deep_mask,
            minimum,
            maximum,
            triangles,
        }))
    }
}

struct TerrainLiquidTriangle {
    vertices: [Vec3; 3],
    cell_bit: u8,
}

fn sample_triangle_height(vertices: [Vec3; 3], point: Vec2) -> Option<f32> {
    let [first, second, third] = vertices;
    let denominator =
        (second.y - third.y) * (first.x - third.x) + (third.x - second.x) * (first.y - third.y);
    if denominator.abs() < 0.000_001 {
        return None;
    }
    let first_weight = ((second.y - third.y) * (point.x - third.x)
        + (third.x - second.x) * (point.y - third.y))
        / denominator;
    let second_weight = ((third.y - first.y) * (point.x - third.x)
        + (first.x - third.x) * (point.y - third.y))
        / denominator;
    let third_weight = 1.0 - first_weight - second_weight;
    if first_weight < -SAMPLE_TOLERANCE
        || second_weight < -SAMPLE_TOLERANCE
        || third_weight < -SAMPLE_TOLERANCE
    {
        return None;
    }
    Some(first_weight * first.z + second_weight * second.z + third_weight * third.z)
}

fn prefer_height(candidate: f32, current: f32, reference: Option<f32>) -> bool {
    let Some(reference) = reference else {
        return candidate > current;
    };
    let candidate_above = candidate >= reference - SAMPLE_TOLERANCE;
    let current_above = current >= reference - SAMPLE_TOLERANCE;
    if candidate_above != current_above {
        return candidate_above;
    }
    if candidate_above {
        candidate < current
    } else {
        candidate > current
    }
}
