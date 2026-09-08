//! MLIQ camera rays using local, retained native mesh coordinates.

use glam::Vec3;
use solarity_asset::WorldModelLiquid;

use super::{PlacedWorldModelCollision, movement_collection::transform_point};
use crate::{PlayerCameraWaterSegment, PlayerCameraWaterSegmentError};

pub(super) struct LiquidRayMesh {
    vertices: Vec<Vec3>,
    bounds: [Vec3; 2],
}

impl LiquidRayMesh {
    pub(super) fn new(grid: &WorldModelLiquid) -> Self {
        let mut vertices = Vec::with_capacity(grid.vertices().len());
        let mut y = grid.corner()[1];
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        for row in 0..grid.vertex_height() {
            let mut x = grid.corner()[0];
            for column in 0..grid.vertex_width() {
                let z = grid.vertices()[(row * grid.vertex_width() + column) as usize].height();
                minimum = minimum.min(z);
                maximum = maximum.max(z);
                vertices.push(Vec3::new(x, y, z));
                x += 4.166_666_5;
            }
            y += 4.166_666_5;
        }
        let upper = [grid.tile_width(), grid.tile_height()].map(|size| {
            (f64::from(size) * f64::from(4.166_666_5_f32) - f64::from(0.1_f32)).max(0.0) as f32
        });
        Self {
            vertices,
            bounds: [
                Vec3::new(grid.corner()[0], grid.corner()[1], minimum),
                Vec3::new(
                    grid.corner()[0] + upper[0],
                    grid.corner()[1] + upper[1],
                    maximum,
                ),
            ],
        }
    }
}

impl PlacedWorldModelCollision {
    /// Traces explicit liquid cells under this retained WMO placement.
    /// All ray normalization and intersections run in native root-local space.
    ///
    /// # Errors
    /// Rejects non-finite inputs, generated geometry, or an invalid maximum.
    pub fn trace_liquid_camera(
        &self,
        start: Vec3,
        end: Vec3,
        maximum: f32,
        cells: &mut Vec<[u16; 2]>,
    ) -> Result<Option<f32>, PlayerCameraWaterSegmentError> {
        let start = transform_point(self.inverse_transform, start);
        let end = transform_point(self.inverse_transform, end);
        let ray = PlayerCameraWaterSegment::new(start, end)?;
        let mut nearest = maximum;
        let mut hit = false;
        for (index, group) in self.model.groups().iter().enumerate() {
            if group.flags() & 0x1000 == 0 || group.resolve_liquid_type(self.model.flags()) == 0 {
                continue;
            }
            let (Some(grid), Some(mesh)) = (group.liquid(), self.liquid_ray_meshes[index].as_ref())
            else {
                continue;
            };
            if clipped_segment(
                start,
                end,
                self.model.group_info()[index]
                    .bounds()
                    .map(Vec3::from_array),
            )
            .is_none()
            {
                continue;
            }
            let Some([first, last]) = clipped_segment(start, end, mesh.bounds) else {
                continue;
            };
            let corner = glam::Vec2::new(grid.corner()[0], grid.corner()[1]);
            let local = |point: Vec3| {
                ((point.truncate().as_dvec2() - corner.as_dvec2())
                    * f64::from(f32::from_bits(0x3e75_c290)))
                .as_vec2()
            };
            crate::camera::water_segment::grid::wmo_cells(
                local(first),
                local(last),
                [grid.tile_width(), grid.tile_height()],
                cells,
            );
            let stride = grid.vertex_width() as usize;
            for &[column, row] in cells.iter() {
                let [column, row] = [usize::from(column), usize::from(row)];
                if column >= grid.tile_width() as usize
                    || row >= grid.tile_height() as usize
                    || grid.tiles()[row * grid.tile_width() as usize + column] & 0xf == 0xf
                {
                    continue;
                }
                let first = row * stride + column;
                for indices in [
                    [first, first + stride + 1, first + stride],
                    [first, first + 1, first + stride + 1],
                ] {
                    if let Some(fraction) =
                        ray.triangle_fraction(indices.map(|i| mesh.vertices[i]), nearest)?
                    {
                        nearest = fraction;
                        hit = true;
                    }
                }
            }
        }
        Ok(hit.then_some(nearest))
    }
}

pub(super) fn clipped_segment(start: Vec3, end: Vec3, bounds: [Vec3; 2]) -> Option<[Vec3; 2]> {
    let origin = start.as_dvec3();
    let delta = end.as_dvec3() - origin;
    let mut first = 0.0_f64;
    let mut last = 1.0_f64;
    for axis in 0..3 {
        if delta[axis] == 0.0 {
            if origin[axis] < f64::from(bounds[0][axis])
                || origin[axis] > f64::from(bounds[1][axis])
            {
                return None;
            }
        } else {
            let a = (f64::from(bounds[0][axis]) - origin[axis]) / delta[axis];
            let b = (f64::from(bounds[1][axis]) - origin[axis]) / delta[axis];
            first = first.max(a.min(b));
            last = last.min(a.max(b));
            if first > last {
                return None;
            }
        }
    }
    Some([
        (origin + delta * first).as_vec3(),
        (origin + delta * last).as_vec3(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liquid_ray_clipping_matches_original_cell_admission()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut actual = Vec::new();
        for line in include_str!("../../tests/fixtures/camera-water-clip-native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let groups = line
                .split('|')
                .map(|group| {
                    group
                        .split_whitespace()
                        .map(|word| u32::from_str_radix(word, 16))
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            let words = &groups[0];
            let float = |i: usize| f32::from_bits(words[i]);
            let start = Vec3::new(float(0), float(1), float(2));
            let end = Vec3::new(float(3), float(4), float(5));
            let corner = glam::Vec2::new(float(6), float(7));
            let dimensions = [words[8], words[9]];
            let upper = dimensions.map(|size| {
                (f64::from(size) * f64::from(4.166_666_5_f32) - f64::from(0.1_f32)).max(0.0) as f32
            });
            let bounds = [
                Vec3::new(corner.x, corner.y, float(10)),
                Vec3::new(corner.x + upper[0], corner.y + upper[1], float(11)),
            ];
            actual.clear();
            if let Some([a, b]) = clipped_segment(start, end, bounds) {
                let local = |p: Vec3| {
                    ((p.truncate().as_dvec2() - corner.as_dvec2())
                        * f64::from(f32::from_bits(0x3e75_c290)))
                    .as_vec2()
                };
                crate::camera::water_segment::grid::wmo_cells(
                    local(a),
                    local(b),
                    dimensions,
                    &mut actual,
                );
            }
            let expected: Vec<_> = groups[1]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| pair.map(|v| v as u16))
                .collect();
            assert_eq!(actual, expected, "{line}");
        }
        Ok(())
    }
}
