//! Hole-aware ADT height-field collision independent of renderer resources.

mod registration;

pub use registration::TerrainRegistrationPoint;

use glam::Vec3;
use solarity_asset::{DecodedTerrainTile, TerrainChunk, TerrainChunkIndex};
use thiserror::Error;

use super::movement_collection::{calculated_triangle, terrain_square_bounds};
use super::{MovementCollectionError, MovementCollisionBounds, MovementCollisionTriangle};

const TERRAIN_SQUARES_PER_CHUNK: usize = 8;
const TERRAIN_UNITS_PER_CHUNK: f32 = 33.333_332;
const TERRAIN_UNIT_SIZE: f32 = TERRAIN_UNITS_PER_CHUNK / TERRAIN_SQUARES_PER_CHUNK as f32;
const BOUNDS_TOLERANCE: f32 = 0.0001;
const CONTACT_TOLERANCE: f32 = 0.0001;
const BARYCENTRIC_TOLERANCE: f32 = 0.0001;

/// Nearest contact along an admitted segment, expressed as a zero-to-one fraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainCollisionHit {
    fraction: f32,
    normal: Vec3,
}

impl TerrainCollisionHit {
    /// Returns the fraction from segment start to end at first contact.
    #[must_use]
    pub const fn fraction(self) -> f32 {
        self.fraction
    }

    /// Returns the upward-facing authored terrain plane normal.
    #[must_use]
    pub const fn normal(self) -> Vec3 {
        self.normal
    }
}

/// Invalid decoded geometry or query input at the terrain collision boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TerrainCollisionError {
    /// A point cannot be represented by the native signed square coordinates.
    #[error("terrain registration point is outside the native grid")]
    OutsideRegistrationGrid,
    /// The supplied point belongs to a different resident ADT.
    #[error("terrain registration point does not belong to this tile")]
    WrongRegistrationTile,
    /// A decoded terrain vertex is NaN or infinite.
    #[error("terrain collision geometry is not finite")]
    NonFiniteGeometry,
    /// A segment endpoint is NaN or infinite.
    #[error("terrain collision segment is not finite")]
    NonFiniteSegment,
    /// A point-height query contains NaN or infinity.
    #[error("terrain collision point is not finite")]
    NonFinitePoint,
    /// Collision radius is negative, NaN, or infinite.
    #[error("terrain collision radius is invalid")]
    InvalidRadius,
    /// Maximum fraction is NaN or infinite.
    #[error("terrain collision maximum fraction is not finite")]
    NonFiniteMaximumFraction,
}

/// Immutable CPU collision geometry for one decoded ADT generation.
pub struct TerrainCollisionMesh {
    chunks: Vec<TerrainCollisionChunk>,
    tile_square_origin: [i32; 2],
}

impl TerrainCollisionMesh {
    /// Builds the exact staggered grid and authored holes for all 256 MCNKs.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainCollisionError::NonFiniteGeometry`] before admitting a
    /// tile whose positions could contaminate broad phase or plane arithmetic.
    pub fn prepare(tile: &DecodedTerrainTile) -> Result<Self, TerrainCollisionError> {
        let chunks = tile
            .chunks()
            .iter()
            .map(TerrainCollisionChunk::prepare)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            chunks,
            tile_square_origin: [
                i32::from(tile.index().y()) * 128,
                i32::from(tile.index().x()) * 128,
            ],
        })
    }

    /// Appends one MCNK's movement faces in stock row, square, and fan order.
    ///
    /// The resident owner calls this in world chunk order, interleaving the
    /// owning chunk's M2 references. This preserves equal-distance contacts
    /// across ADT boundaries. The caller retains ownership of output storage.
    ///
    /// # Errors
    /// Returns [`MovementCollectionError`] if selected geometry is invalid.
    pub fn append_movement_chunk(
        &self,
        index: TerrainChunkIndex,
        bounds: MovementCollisionBounds,
        output: &mut Vec<MovementCollisionTriangle>,
    ) -> Result<(), MovementCollectionError> {
        let chunk = &self.chunks[usize::from(index.y()) * 16 + usize::from(index.x())];
        let origin = [
            self.tile_square_origin[0] + i32::from(index.y()) * 8,
            self.tile_square_origin[1] + i32::from(index.x()) * 8,
        ];
        let [minimum, maximum] = terrain_square_bounds(bounds)?;
        let minimum = [minimum[0] - origin[0], minimum[1] - origin[1]];
        let maximum = [maximum[0] - origin[0], maximum[1] - origin[1]];
        let local_bounds =
            MovementCollisionBounds::new(bounds.minimum - chunk.base, bounds.maximum - chunk.base)?;
        for row in minimum[0].max(0)..=maximum[0].min(7) {
            for column in minimum[1].max(0)..=maximum[1].min(7) {
                if chunk.holes & (1 << ((row / 2) * 4 + column / 2)) != 0 {
                    continue;
                }
                let start = row as usize * 17 + column as usize;
                for offsets in [[17, 9, 0], [9, 1, 0], [9, 17, 18], [9, 18, 1]] {
                    let vertices = offsets.map(|offset| chunk.local_vertices[start + offset]);
                    if local_bounds.admits(vertices, f64::from(0.019_444_443_f32)) {
                        output.push(calculated_triangle(vertices.map(|p| p + chunk.base))?);
                    }
                }
            }
        }
        Ok(())
    }

    /// Traces the upper side of the ADT height field and returns nearest contact.
    ///
    /// The horizontal radius uses stock's world-axis-aligned box support. A
    /// zero radius is the ray form consumed by ordinary camera obstruction.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainCollisionError`] for non-finite endpoints, radius, or
    /// maximum fraction. Valid fractions are clamped to the segment domain.
    pub fn trace(
        &self,
        start: Vec3,
        end: Vec3,
        collision_radius: f32,
        maximum_fraction: f32,
    ) -> Result<Option<TerrainCollisionHit>, TerrainCollisionError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(TerrainCollisionError::NonFiniteSegment);
        }
        if !collision_radius.is_finite() || collision_radius < 0.0 {
            return Err(TerrainCollisionError::InvalidRadius);
        }
        if !maximum_fraction.is_finite() {
            return Err(TerrainCollisionError::NonFiniteMaximumFraction);
        }
        let maximum_fraction = maximum_fraction.clamp(0.0, 1.0);
        let delta = end - start;
        if delta.length_squared() < 1.0e-12 || maximum_fraction <= 0.0 {
            return Ok(None);
        }

        let limited_end = start + delta * maximum_fraction;
        let query_minimum = start.min(limited_end) - Vec3::splat(collision_radius);
        let query_maximum = start.max(limited_end) + Vec3::splat(collision_radius);
        let mut nearest = None;
        let mut nearest_fraction = maximum_fraction;
        for chunk in &self.chunks {
            if query_maximum.x < chunk.minimum.x - BOUNDS_TOLERANCE
                || query_minimum.x > chunk.maximum.x + BOUNDS_TOLERANCE
                || query_maximum.y < chunk.minimum.y - BOUNDS_TOLERANCE
                || query_minimum.y > chunk.maximum.y + BOUNDS_TOLERANCE
            {
                continue;
            }
            chunk.trace(
                start,
                delta,
                collision_radius,
                &mut nearest_fraction,
                &mut nearest,
            );
        }
        Ok(nearest)
    }

    /// Samples the highest authored terrain triangle at one world-space point.
    ///
    /// This is the point-height form consumed by movement support queries. It
    /// uses the same hole-filtered triangle topology as segment collision and
    /// does not depend on a guessed vertical ray extent.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainCollisionError::NonFinitePoint`] for a non-finite
    /// horizontal coordinate.
    pub fn height_at(
        &self,
        world_x: f32,
        world_y: f32,
    ) -> Result<Option<f32>, TerrainCollisionError> {
        if !world_x.is_finite() || !world_y.is_finite() {
            return Err(TerrainCollisionError::NonFinitePoint);
        }
        let mut height: Option<f32> = None;
        for chunk in &self.chunks {
            if world_x < chunk.minimum.x - BOUNDS_TOLERANCE
                || world_x > chunk.maximum.x + BOUNDS_TOLERANCE
                || world_y < chunk.minimum.y - BOUNDS_TOLERANCE
                || world_y > chunk.maximum.y + BOUNDS_TOLERANCE
            {
                continue;
            }
            if let Some(candidate) = chunk.height_at(world_x, world_y)
                && height.is_none_or(|current| candidate > current)
            {
                height = Some(candidate);
            }
        }
        Ok(height)
    }
}

/// One MCNK's compact collision triangles and XY broad-phase extent.
struct TerrainCollisionChunk {
    vertices: Vec<Vec3>,
    local_vertices: Vec<Vec3>,
    base: Vec3,
    holes: u16,
    indices: Vec<u16>,
    minimum: Vec3,
    maximum: Vec3,
}

impl TerrainCollisionChunk {
    /// Expands relative MCVT values into the same world positions as rendering.
    fn prepare(chunk: &TerrainChunk) -> Result<Self, TerrainCollisionError> {
        let base = Vec3::from_array(chunk.position());
        let mut vertices = Vec::with_capacity(145);
        let mut local_vertices = Vec::with_capacity(145);
        for logical_row in 0..17 {
            let inner = logical_row % 2 == 1;
            let column_count = if inner { 8 } else { 9 };
            for column in 0..column_count {
                let source = interleaved_vertex_index(logical_row, column);
                let row_units = logical_row as f32 * 0.5;
                let column_units = column as f32 + if inner { 0.5 } else { 0.0 };
                let position = Vec3::new(
                    base.x - row_units * TERRAIN_UNIT_SIZE,
                    base.y - column_units * TERRAIN_UNIT_SIZE,
                    base.z + chunk.heights()[source],
                );
                if !position.is_finite() {
                    return Err(TerrainCollisionError::NonFiniteGeometry);
                }
                vertices.push(position);
                local_vertices.push(Vec3::new(
                    -row_units * TERRAIN_UNIT_SIZE,
                    -column_units * TERRAIN_UNIT_SIZE,
                    chunk.heights()[source],
                ));
            }
        }
        let indices = prepare_indices(chunk.holes());
        let (minimum, maximum) = vertices.iter().copied().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), position| (minimum.min(position), maximum.max(position)),
        );
        Ok(Self {
            vertices,
            local_vertices,
            base,
            holes: chunk.holes(),
            indices,
            minimum,
            maximum,
        })
    }

    /// Refines broad-phase admission against every non-hole terrain triangle.
    fn trace(
        &self,
        start: Vec3,
        delta: Vec3,
        collision_radius: f32,
        nearest_fraction: &mut f32,
        nearest: &mut Option<TerrainCollisionHit>,
    ) {
        let (triangles, remainder) = self.indices.as_chunks::<3>();
        debug_assert!(remainder.is_empty());
        for triangle in triangles {
            let first = self.vertices[usize::from(triangle[0])];
            let second = self.vertices[usize::from(triangle[1])];
            let third = self.vertices[usize::from(triangle[2])];
            let mut normal = (second - first).cross(third - first);
            let normal_length_squared = normal.length_squared();
            if normal_length_squared < 1.0e-12 {
                continue;
            }
            normal /= normal_length_squared.sqrt();
            if normal.z < 0.0 {
                normal = -normal;
            }
            let delta_into_plane = delta.dot(normal);
            if delta_into_plane >= -CONTACT_TOLERANCE {
                continue;
            }
            let denominator = (second.y - third.y) * (first.x - third.x)
                + (third.x - second.x) * (first.y - third.y);
            if denominator.abs() < 0.000_001 {
                continue;
            }
            let center_start_distance = (start - first).dot(normal);
            if center_start_distance < -CONTACT_TOLERANCE {
                continue;
            }

            let footprint_offset = Vec3::new(
                if normal.x > 0.0 {
                    -collision_radius
                } else {
                    collision_radius
                },
                if normal.y > 0.0 {
                    -collision_radius
                } else {
                    collision_radius
                },
                0.0,
            );
            let horizontal_support = collision_radius * (normal.x.abs() + normal.y.abs());
            let fraction =
                ((center_start_distance - horizontal_support) / -delta_into_plane).clamp(0.0, 1.0);
            if fraction > *nearest_fraction + BOUNDS_TOLERANCE {
                continue;
            }
            let point = start + delta * fraction + footprint_offset;
            let first_weight = ((second.y - third.y) * (point.x - third.x)
                + (third.x - second.x) * (point.y - third.y))
                / denominator;
            let second_weight = ((third.y - first.y) * (point.x - third.x)
                + (first.x - third.x) * (point.y - third.y))
                / denominator;
            let third_weight = 1.0 - first_weight - second_weight;
            if first_weight < -BARYCENTRIC_TOLERANCE
                || second_weight < -BARYCENTRIC_TOLERANCE
                || third_weight < -BARYCENTRIC_TOLERANCE
            {
                continue;
            }
            *nearest_fraction = fraction;
            *nearest = Some(TerrainCollisionHit { fraction, normal });
        }
    }

    fn height_at(&self, world_x: f32, world_y: f32) -> Option<f32> {
        let mut height: Option<f32> = None;
        let (triangles, remainder) = self.indices.as_chunks::<3>();
        debug_assert!(remainder.is_empty());
        for triangle in triangles {
            let first = self.vertices[usize::from(triangle[0])];
            let second = self.vertices[usize::from(triangle[1])];
            let third = self.vertices[usize::from(triangle[2])];
            let denominator = (second.y - third.y) * (first.x - third.x)
                + (third.x - second.x) * (first.y - third.y);
            if denominator.abs() < 0.000_001 {
                continue;
            }
            let first_weight = ((second.y - third.y) * (world_x - third.x)
                + (third.x - second.x) * (world_y - third.y))
                / denominator;
            let second_weight = ((third.y - first.y) * (world_x - third.x)
                + (first.x - third.x) * (world_y - third.y))
                / denominator;
            let third_weight = 1.0 - first_weight - second_weight;
            if first_weight < -BARYCENTRIC_TOLERANCE
                || second_weight < -BARYCENTRIC_TOLERANCE
                || third_weight < -BARYCENTRIC_TOLERANCE
            {
                continue;
            }
            let candidate =
                first.z * first_weight + second.z * second_weight + third.z * third_weight;
            if height.is_none_or(|current| candidate > current) {
                height = Some(candidate);
            }
        }
        height
    }
}

/// Generates the stock four-triangle fan for each non-hole terrain square.
fn prepare_indices(holes: u16) -> Vec<u16> {
    let mut indices = Vec::with_capacity(8 * 8 * 4 * 3);
    for row in 0..TERRAIN_SQUARES_PER_CHUNK {
        for column in 0..TERRAIN_SQUARES_PER_CHUNK {
            let hole_bit = (row / 2) * 4 + column / 2;
            if holes & (1 << hole_bit) != 0 {
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

const fn vertex_index(logical_row: usize, column: usize) -> u16 {
    interleaved_vertex_index(logical_row, column) as u16
}

const fn interleaved_vertex_index(logical_row: usize, column: usize) -> usize {
    logical_row.div_ceil(2) * 9 + (logical_row / 2) * 8 + column
}
