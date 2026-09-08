//! Native normalized liquid ray (7A3570/7C8DD0) and indexed triangle (9836B0).

use glam::Vec3;
use thiserror::Error;

pub(crate) mod grid;

/// A camera water ray in its owning terrain-chunk or WMO coordinate space.
pub struct PlayerCameraWaterSegment {
    start: Vec3,
    end: Vec3,
    direction: Vec3,
    inverse_length: f32,
}

/// Non-finite camera water geometry or a fraction outside the segment.
#[derive(Debug, Error)]
#[error("camera water segment or triangle is invalid")]
pub struct PlayerCameraWaterSegmentError;

impl PlayerCameraWaterSegment {
    /// Collects crossed global terrain cells as world-X/world-Y indices.
    /// The runtime maps Y to the ADT filename X and X to its filename Y.
    pub fn terrain_cells(&self, output: &mut Vec<[u16; 2]>) {
        grid::terrain_cells(self.start.truncate(), self.end.truncate(), output);
    }

    /// Traces the MH2O layers in one crossed MCNK-local cell.
    ///
    /// # Errors
    /// Rejects malformed cell coordinates, liquid geometry or maximum fraction.
    pub fn terrain_cell_fraction(
        &self,
        tile: &solarity_asset::DecodedTerrainTile,
        chunk: solarity_asset::TerrainChunkIndex,
        cell: [u16; 2],
        maximum: f32,
    ) -> Result<Option<f32>, PlayerCameraWaterSegmentError> {
        if cell.iter().any(|&v| v >= 8) {
            return Err(PlayerCameraWaterSegmentError);
        }
        let Some(liquids) = tile.liquids() else {
            return Ok(None);
        };
        let chunk_index = usize::from(chunk.y()) * 16 + usize::from(chunk.x());
        let base = Vec3::from_array(tile.chunks()[chunk_index].position());
        let local = Self {
            start: self.start - base,
            end: self.end - base,
            direction: self.direction,
            inverse_length: self.inverse_length,
        };
        let mut nearest = maximum;
        let mut hit = false;
        let [row, column] = cell.map(usize::from);
        for layer in liquids.chunks()[chunk_index].layers() {
            let first_row = usize::from(layer.y_offset());
            let first_column = usize::from(layer.x_offset());
            let width = usize::from(layer.width());
            let height = usize::from(layer.height());
            if row < first_row
                || row >= first_row + height
                || column < first_column
                || column >= first_column + width
            {
                continue;
            }
            if layer.exists()[(row - first_row) * width + column - first_column] == 0 {
                continue;
            }
            let stride = width + 1;
            let first = (row - first_row) * stride + column - first_column;
            let vertices = [
                (row, column, first),
                (row + 1, column, first + stride),
                (row + 1, column + 1, first + stride + 1),
                (row, column + 1, first + 1),
            ]
            .map(|(row, column, index)| {
                Vec3::new(
                    row as f32 * -4.166_666_5,
                    column as f32 * -4.166_666_5,
                    layer.heights()[index] - base.z,
                )
            });
            // 7A3570's indexed order matters at tolerant shared edges.
            for indices in [[1, 2, 0], [3, 2, 0]] {
                if let Some(fraction) =
                    local.triangle_fraction(indices.map(|i| vertices[i]), nearest)?
                {
                    nearest = fraction;
                    hit = true;
                }
            }
        }
        Ok(hit.then_some(nearest))
    }
    /// Normalizes the segment while retaining the native reciprocal store.
    ///
    /// # Errors
    /// Rejects non-finite endpoints. A zero-length segment admits no hits.
    pub fn new(start: Vec3, end: Vec3) -> Result<Self, PlayerCameraWaterSegmentError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(PlayerCameraWaterSegmentError);
        }
        let delta = end.as_dvec3() - start.as_dvec3();
        let inverse = ((delta.z * delta.z + delta.y * delta.y) + delta.x * delta.x)
            .sqrt()
            .recip();
        Ok(Self {
            start,
            end,
            direction: (delta * inverse).as_vec3(),
            inverse_length: inverse as f32,
        })
    }

    /// Intersects an admitted liquid triangle. The far endpoint and ties with
    /// an earlier owner are excluded; the segment's start is included.
    ///
    /// # Errors
    /// Rejects non-finite triangle vertices or an invalid maximum fraction.
    pub fn triangle_fraction(
        &self,
        vertices: [Vec3; 3],
        maximum: f32,
    ) -> Result<Option<f32>, PlayerCameraWaterSegmentError> {
        if vertices.iter().any(|vertex| !vertex.is_finite())
            || !maximum.is_finite()
            || !(0.0..=1.0).contains(&maximum)
        {
            return Err(PlayerCameraWaterSegmentError);
        }
        if !self.direction.is_finite() || !self.inverse_length.is_finite() {
            return Ok(None);
        }
        let Some(distance) = triangle_distance(self.start, self.direction, vertices) else {
            return Ok(None);
        };
        let fraction = (f64::from(distance) * f64::from(self.inverse_length)) as f32;
        Ok((fraction >= 0.0 && fraction < maximum).then_some(fraction))
    }

    /// Reconstructs the native 77F310 world contact at a resolved fraction.
    #[must_use]
    pub fn contact(&self, fraction: f32) -> Vec3 {
        (self.start.as_dvec3()
            + (self.end.as_dvec3() - self.start.as_dvec3()) * f64::from(fraction))
        .as_vec3()
    }
}

fn triangle_distance(start: Vec3, direction: Vec3, vertices: [Vec3; 3]) -> Option<f32> {
    let [a, b, c] = vertices.map(Vec3::as_dvec3);
    let first = b - a;
    let second = c - a;
    let direction = direction.as_dvec3();
    let mut cross = direction.cross(second);
    cross.x = f64::from(cross.x as f32);
    let determinant = (cross.z * first.z + cross.y * first.y) + cross.x * first.x;
    if determinant.abs() < f64::from(0.000_001_f32) {
        return None;
    }
    let inverse = f64::from(determinant.recip() as f32);
    let offset = start.as_dvec3() - a;
    let u = ((offset.z * cross.z + offset.y * cross.y) + offset.x * cross.x) * inverse;
    let tolerance = f64::from(0.01_f32);
    let maximum = f64::from((1.0 + tolerance) as f32);
    if u < -tolerance || u > maximum {
        return None;
    }
    let first = glam::DVec3::new(first.x, first.y, f64::from(first.z as f32));
    let cross = offset.cross(first);
    let v = ((direction.z * cross.z + direction.y * cross.y) + direction.x * cross.x) * inverse;
    if v < -tolerance || v + f64::from(u as f32) > maximum {
        return None;
    }
    let second = second.as_vec3().as_dvec3();
    Some((((cross.z * second.z + cross.y * second.y) + cross.x * second.x) * inverse) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liquid_segments_match_original_wmo_cell_kernel() -> Result<(), Box<dyn std::error::Error>> {
        for line in include_str!("../../tests/fixtures/camera-water-segment-native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let words = line
                .split_whitespace()
                .filter(|word| *word != "|")
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()?;
            let point = |offset: usize| {
                Vec3::new(
                    f32::from_bits(words[offset]),
                    f32::from_bits(words[offset + 1]),
                    f32::from_bits(words[offset + 2]),
                )
            };
            let segment = PlayerCameraWaterSegment::new(point(0), point(3))?;
            let mut maximum = f32::from_bits(words[6]);
            let vertices = [point(7), point(10), point(13), point(16)];
            let mut hit = false;
            if words[19] & 0xf != 0xf {
                for indices in [[0, 3, 2], [0, 1, 3]] {
                    if let Some(fraction) =
                        segment.triangle_fraction(indices.map(|i| vertices[i]), maximum)?
                    {
                        maximum = fraction;
                        hit = true;
                    }
                }
            }
            // Native zero-length normalization can report a NaN hit. The
            // portable boundary deliberately keeps that invalid result out.
            if !f32::from_bits(words[21]).is_finite() {
                assert!(!hit, "{line}");
                continue;
            }
            assert_eq!(hit, words[20] != 0, "{line}");
            assert!(
                (maximum - f32::from_bits(words[21])).abs() <= 0.000_002,
                "{line}: {maximum}"
            );
        }
        Ok(())
    }
}
