//! Direct native terrain registration sample (`0x007C1660/0x007AD3B0`).

use glam::{DVec3, Vec3};
use solarity_asset::{TerrainChunkIndex, TerrainTileIndex};

use super::{TerrainCollisionChunk, TerrainCollisionError, TerrainCollisionMesh};
use crate::{MovementCollisionBounds, MovementTerrainChunks};

const SPACING: f64 = -4.166_666_5_f32 as f64;

impl MovementCollisionBounds {
    /// Visits spatial-registration chunks in native `0x007C2040` order.
    ///
    /// This uses the registration routine's direct chunk conversion, including
    /// its float stores and wrapped map addresses. The caller resolves tile
    /// residency and admits only chunks whose minimum Z is at most the object's
    /// maximum Z. Movement collection uses its separate square conversion.
    ///
    /// # Errors
    /// Returns [`TerrainCollisionError::OutsideRegistrationGrid`] if native
    /// integer conversion cannot represent a coordinate.
    pub fn registration_terrain_chunks(
        self,
    ) -> Result<MovementTerrainChunks, TerrainCollisionError> {
        let origin = f64::from(17_066.666_f32);
        let grid = |value: f64| {
            let scaled = (value * f64::from(0.03_f32)) as f32;
            let rounded = (f64::from(scaled) - 0.5).round_ties_even();
            if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
                Err(TerrainCollisionError::OutsideRegistrationGrid)
            } else {
                Ok(rounded as i32)
            }
        };
        let first = [
            grid(f64::from((origin - f64::from(self.maximum.x)) as f32))?,
            grid(f64::from((origin - f64::from(self.maximum.y)) as f32))?,
        ];
        let last = [
            grid(f64::from((origin - f64::from(self.minimum.x)) as f32))?,
            grid(origin - f64::from(self.minimum.y))?,
        ];
        Ok(MovementTerrainChunks::from_grid_bounds(first, last))
    }
}

/// One native point-to-square conversion with its exact owning ADT and MCNK.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainRegistrationPoint {
    world: [f32; 2],
    squares: [i32; 2],
    tile: TerrainTileIndex,
    chunk: TerrainChunkIndex,
}

impl TerrainRegistrationPoint {
    /// Selects a native global square, preserving float stores and ties-to-even.
    ///
    /// The original masked ADT/MCNK addresses are retained at map boundaries.
    /// Residency must be resolved for this exact address before sampling height.
    ///
    /// # Errors
    /// Returns [`TerrainCollisionError`] for non-finite or unrepresentable input.
    pub fn new(world_x: f32, world_y: f32) -> Result<Self, TerrainCollisionError> {
        let world = [world_x, world_y];
        if world.iter().any(|v| !v.is_finite()) {
            return Err(TerrainCollisionError::NonFinitePoint);
        }
        let mut squares = [0; 2];
        for (index, value) in world.into_iter().enumerate() {
            // Unlike the box collector, 7C1660 does not spill the subtraction.
            let scaled =
                ((f64::from(17_066.666_f32) - f64::from(value)) * f64::from(0.24_f32)) as f32;
            let rounded = (f64::from(scaled) - 0.5).round_ties_even();
            if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
                return Err(TerrainCollisionError::OutsideRegistrationGrid);
            }
            squares[index] = rounded as i32;
        }
        let tile = TerrainTileIndex::new(
            ((squares[1] >> 7) & 63) as u8,
            ((squares[0] >> 7) & 63) as u8,
        )
        .ok_or(TerrainCollisionError::OutsideRegistrationGrid)?;
        let chunk = TerrainChunkIndex::new(
            ((squares[1] >> 3) & 15) as u8,
            ((squares[0] >> 3) & 15) as u8,
        )
        .ok_or(TerrainCollisionError::OutsideRegistrationGrid)?;
        Ok(Self {
            world,
            squares,
            tile,
            chunk,
        })
    }

    /// Returns the ADT whose residency the scene must resolve.
    #[must_use]
    pub const fn tile(self) -> TerrainTileIndex {
        self.tile
    }

    /// Returns the MCNK inside the selected ADT.
    #[must_use]
    pub const fn chunk(self) -> TerrainChunkIndex {
        self.chunk
    }
}

impl TerrainCollisionMesh {
    /// Samples the single native registration triangle in the selected MCNK.
    ///
    /// Authored holes return no height. Native CPU normalization and float
    /// stores are independent of the broader support/camera sampling methods.
    ///
    /// # Errors
    /// Returns [`TerrainCollisionError`] for the wrong tile or invalid geometry.
    pub fn registration_height_at(
        &self,
        point: TerrainRegistrationPoint,
    ) -> Result<Option<f32>, TerrainCollisionError> {
        if i32::from(point.tile.y()) * 128 != self.tile_square_origin[0]
            || i32::from(point.tile.x()) * 128 != self.tile_square_origin[1]
        {
            return Err(TerrainCollisionError::WrongRegistrationTile);
        }
        let chunk = &self.chunks[usize::from(point.chunk.y()) * 16 + usize::from(point.chunk.x())];
        let sse = solarity_cpu::reciprocal_sqrt_estimate(1.0).is_some();
        registration_height(
            chunk,
            point.world,
            point.squares.map(|v| (v & 7) as usize),
            sse,
        )
    }

    /// Returns a resident chunk's authored-geometry bounds for spatial registration.
    #[must_use]
    pub fn chunk_bounds(&self, index: TerrainChunkIndex) -> crate::MovementCollisionBounds {
        let chunk = &self.chunks[usize::from(index.y()) * 16 + usize::from(index.x())];
        crate::MovementCollisionBounds {
            minimum: chunk.minimum,
            maximum: chunk.maximum,
        }
    }
}

fn registration_height(
    chunk: &TerrainCollisionChunk,
    world: [f32; 2],
    square: [usize; 2],
    sse: bool,
) -> Result<Option<f32>, TerrainCollisionError> {
    let [row, column] = square;
    if chunk.holes & (1 << ((row / 2) * 4 + column / 2)) != 0 {
        return Ok(None);
    }
    let x = row as f64 * SPACING;
    let y = column as f64 * SPACING;
    let point_x = f64::from(world[0] - chunk.base.x);
    let point_y = f64::from(world[1] - chunk.base.y);
    let diagonal = ((y + SPACING) - point_y) * ((x + SPACING) - x)
        - ((x + SPACING) - point_x) * (f64::from((y + SPACING) as f32) - f64::from(y as f32));
    let opposite = (y - point_y) * ((x + SPACING) - x)
        - ((x + SPACING) - point_x) * (y - f64::from((y + SPACING) as f32));
    let selected = usize::from(diagonal <= 0.0) + 2 * usize::from(opposite <= 0.0);
    let corners = [[0., 0.], [0., SPACING], [SPACING, SPACING], [SPACING, 0.]];
    let pairs = [[3, 0], [0, 1], [2, 3], [1, 2]][selected];
    let offsets = [[17, 0], [0, 1], [18, 17], [1, 18]][selected];
    let first = row * 17 + column;
    let center = DVec3::new(
        x + SPACING * 0.5,
        y + SPACING * 0.5,
        f64::from(chunk.local_vertices[first + 9].z),
    );
    let vertices = std::array::from_fn::<_, 2, _>(|i| {
        DVec3::new(
            f64::from((x + corners[pairs[i]][0]) as f32),
            f64::from((y + corners[pairs[i]][1]) as f32),
            f64::from(chunk.local_vertices[first + offsets[i]].z),
        )
    });
    let height = if sse {
        sse_height(center, vertices, [point_x, point_y], chunk.base.z)
    } else {
        scalar_height(center.as_vec3(), vertices, [point_x, point_y], chunk.base.z)
    };
    if !height.is_finite() {
        return Err(TerrainCollisionError::NonFiniteGeometry);
    }
    Ok(Some(height))
}

fn scalar_height(center: Vec3, vertices: [DVec3; 2], point: [f64; 2], base_z: f32) -> f32 {
    let center = center.as_dvec3();
    let first = vertices[0] - center;
    let second = vertices[1] - center;
    let cross = first.cross(second).as_vec3().as_dvec3();
    let length = ((cross.x * cross.x + cross.y * cross.y) + cross.z * cross.z).sqrt();
    let normal = cross / length;
    let distance = -((center.x * normal.x + center.y * normal.y) + center.z * normal.z) as f32;
    let normal = normal.as_vec3().as_dvec3();
    (f64::from(base_z)
        - ((normal.x * point[0] + normal.y * point[1]) + f64::from(distance)) / normal.z) as f32
}

fn sse_height(center: DVec3, vertices: [DVec3; 2], point: [f64; 2], base_z: f32) -> f32 {
    let first = vertices[0] - center;
    let second = vertices[1] - center;
    let cross_x = f64::from((first.y * second.z - first.z * second.y) as f32);
    let cross_y = f64::from(first.z as f32) * f64::from(second.x as f32)
        - f64::from(second.z as f32) * first.x;
    let cross_z = first.x * second.y - f64::from(second.x as f32) * first.y;
    let [x, y, z] = [cross_x, cross_y, cross_z].map(|v| v as f32);
    let squared = ((y * y + z * z) + 0.0) + x * x;
    let reciprocal = solarity_cpu::reciprocal_sqrt_estimate(squared)
        .map_or_else(|| f64::from(squared).sqrt().recip(), f64::from);
    let normal = DVec3::new(cross_x, cross_y, cross_z) * reciprocal;
    let distance = -((center.x * normal.x + center.y * normal.y) + center.z * normal.z);
    (f64::from(base_z) - ((normal.x * point[0] + normal.y * point[1]) + distance) / normal.z) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_registration_matches_original_traversal_and_rounding()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut count = 0;
        for line in include_str!("../../../tests/fixtures/terrain-registration-chunks-native.txt")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            let words = line.split_whitespace().collect::<Vec<_>>();
            let values = words[..7]
                .iter()
                .map(|v| u32::from_str_radix(v, 16).map(f32::from_bits))
                .collect::<Result<Vec<_>, _>>()?;
            // These cases expose every visited grid address; separate oracle
            // cases retain the native unavailable-tile and height-filter evidence.
            if words[9] != "0" || values[6] > values[5] {
                continue;
            }
            let bounds = MovementCollisionBounds::new(
                Vec3::from_slice(&values[..3]),
                Vec3::from_slice(&values[3..6]),
            )?;
            let expected = words[13..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    let row = u32::from_str_radix(pair[0], 16)?;
                    let column = u32::from_str_radix(pair[1], 16)?;
                    Ok::<_, std::num::ParseIntError>([
                        ((column >> 4) & 63) as u8,
                        ((row >> 4) & 63) as u8,
                        (column & 15) as u8,
                        (row & 15) as u8,
                    ])
                })
                .collect::<Result<Vec<_>, _>>()?;
            let actual = bounds
                .registration_terrain_chunks()?
                .map(|(tile, chunk)| [tile.x(), tile.y(), chunk.x(), chunk.y()])
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{line}");
            count += 1;
        }
        assert_eq!(count, 256);
        Ok(())
    }

    #[test]
    fn grid_matches_original_float_stores_and_wrapped_boundary_addresses()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut count = 0;
        for line in include_str!("../../../tests/fixtures/terrain-registration-grid-native.txt")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            let values = line
                .split_whitespace()
                .map(|v| u32::from_str_radix(v, 16))
                .collect::<Result<Vec<_>, _>>()?;
            let point = TerrainRegistrationPoint::new(
                f32::from_bits(values[0]),
                f32::from_bits(values[1]),
            )?;
            assert_eq!(
                point.squares,
                [values[2] as i32, values[3] as i32],
                "{line}"
            );
            assert_eq!(
                [point.tile.y(), point.tile.x()],
                [(values[2] >> 7 & 63) as u8, (values[3] >> 7 & 63) as u8],
                "{line}"
            );
            assert_eq!(
                [point.chunk.y(), point.chunk.x()],
                [(values[2] >> 3 & 15) as u8, (values[3] >> 3 & 15) as u8],
                "{line}"
            );
            count += 1;
        }
        assert_eq!(count, 268);
        Ok(())
    }

    #[test]
    fn height_matches_original_triangle_choice_holes_and_cpu_math()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut count = 0;
        for line in include_str!("../../../tests/fixtures/terrain-registration-height-native.txt")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            let words = line.split_whitespace().collect::<Vec<_>>();
            let values = words[..5]
                .iter()
                .map(|v| u32::from_str_radix(v, 16).map(f32::from_bits))
                .collect::<Result<Vec<_>, _>>()?;
            let row = words[5].parse::<usize>()?;
            let column = words[6].parse::<usize>()?;
            let profile = words[7].parse::<usize>()?;
            let sse = words[8] == "1";
            let heights = (0..145)
                .map(|i| {
                    (match profile {
                        0 => 0.,
                        1 => ((i * 17) % 31) as f64 / 7.,
                        2 => (i % 17) as f64 * 0.1234567 + (i / 17) as f64 * 0.7654321,
                        _ => 10000. + ((i * 37) % 113) as f64 / 13.,
                    }) as f32
                })
                .collect::<Vec<_>>();
            let chunk = TerrainCollisionChunk {
                vertices: Vec::new(),
                local_vertices: heights.into_iter().map(|h| Vec3::new(0., 0., h)).collect(),
                base: Vec3::from_slice(&values[..3]),
                holes: words[9].parse()?,
                indices: Vec::new(),
                minimum: Vec3::ZERO,
                maximum: Vec3::ZERO,
            };
            let result = registration_height(&chunk, [values[3], values[4]], [row, column], sse)?;
            assert_eq!(result.is_some(), words[10] == "1", "{line}");
            if let Some(height) = result {
                assert_eq!(
                    height.to_bits(),
                    u32::from_str_radix(words[11], 16)?,
                    "{line}"
                );
            }
            count += 1;
        }
        assert_eq!(count, 258);
        Ok(())
    }
}
