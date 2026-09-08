//! Native WMO floor color interpolation (7C7FE0) and entity color split (7C1AD0).

use glam::Vec3;

use super::{PlacedWorldModelCollision, WorldModelCollisionError};

/// Packed floor illumination and the polygon's exterior-daylight blend flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldModelFloorLight {
    color: [u8; 4],
    exterior: bool,
}

impl WorldModelFloorLight {
    /// Returns the native interpolated BGRA color, including daylight blend alpha.
    #[must_use]
    pub const fn color(self) -> [u8; 4] {
        self.color
    }

    /// Returns MOPY bit zero, which enables the entity's daylight color/ray blend.
    #[must_use]
    pub const fn blends_exterior(self) -> bool {
        self.exterior
    }

    /// Returns the native diffuse and ambient BGRA targets (maxima 168 and 96).
    #[must_use]
    pub fn split(self) -> [[u8; 4]; 2] {
        split_color(self.color, 168)
    }
}

/// Splits MODD's authored color with its distinct native diffuse minimum (112).
#[must_use]
pub fn world_model_doodad_light_colors(color: [u8; 4]) -> [[u8; 4]; 2] {
    split_color(color, 112)
}

impl PlacedWorldModelCollision {
    /// Samples a registered floor face using current placement and shared MOCV fixup.
    /// A portal-selected/missing face uses MOHD ambient, as native 7C7FE0 does.
    /// Exterior groups and groups without MOCV leave the entity's prior color alone.
    ///
    /// # Errors
    /// Rejects invalid group indices and nonfinite positions.
    pub fn sample_group_floor_light(
        &self,
        group_index: usize,
        face: Option<u16>,
        world_position: Vec3,
    ) -> Result<Option<WorldModelFloorLight>, WorldModelCollisionError> {
        if !world_position.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        let group = self
            .model
            .groups()
            .get(group_index)
            .ok_or(WorldModelCollisionError::InvalidGroup { group_index })?;
        if group.flags() & 0x48 != 0 || group.vertex_colors().is_empty() {
            return Ok(None);
        }
        let Some(face) = face.filter(|&face| usize::from(face) < group.polygons().len()) else {
            return Ok(Some(WorldModelFloorLight {
                color: self.model.ambient_color(),
                exterior: false,
            }));
        };
        let index = usize::from(face);
        let indices: [usize; 3] =
            std::array::from_fn(|i| usize::from(group.indices()[index * 3 + i]));
        let vertices = indices.map(|index| Vec3::from_array(group.vertices()[index]));
        let colors = indices.map(|index| {
            group
                .fixed_vertex_color(self.model.flags(), index)
                .unwrap_or([255; 4])
        });
        let position =
            super::movement_collection::transform_point(self.inverse_transform, world_position);
        Ok(interpolate(
            vertices,
            position,
            colors,
            self.model.ambient_color(),
            self.model.flags(),
        )
        .map(|color| WorldModelFloorLight {
            color,
            exterior: group.polygons()[index].flags() & 1 != 0,
        }))
    }
}

fn interpolate(
    vertices: [Vec3; 3],
    position: Vec3,
    colors: [[u8; 4]; 3],
    ambient: [u8; 4],
    flags: u16,
) -> Option<[u8; 4]> {
    let [a, b, c] = vertices.map(Vec3::as_dvec3);
    let normal = (c - a).cross(b - a).as_vec3().abs();
    // 9829B0 resolves equal components toward the later axis.
    let axis = if normal.x > normal.y {
        if normal.x > normal.z { 0 } else { 2 }
    } else if normal.y > normal.z {
        1
    } else {
        2
    };
    let [x, y] = [[1, 2], [2, 0], [0, 1]][axis];
    let edge_b = b - a;
    let edge_c = c - a;
    let point = position.as_dvec3() - a;
    let determinant = edge_c[y] * edge_b[x] - edge_b[y] * edge_c[x];
    if determinant == 0.0 || !determinant.is_finite() {
        return None;
    }
    let inverse = determinant.recip();
    let wb = (edge_c[y] * point[x] - point[y] * edge_c[x]) * inverse;
    let wc = ((point[y] * edge_b[x] - edge_b[y] * point[x]) * inverse) as f32;
    let mut wb = (f64::from((wb * 256.0) as f32) - 0.5).round_ties_even() as i32;
    let mut wc = (f64::from(wc * 256.0) - 0.5).round_ties_even() as i32;
    let mut wa = 256_i32.wrapping_sub(wc).wrapping_sub(wb);
    // Native signed integer redistribution projects points outside the triangle.
    if wb < 0 {
        let adjustment = wc.wrapping_mul(wb).checked_div(wc.wrapping_add(wa))?;
        wc = wc.wrapping_add(adjustment);
        wa = wa.wrapping_add(wb.wrapping_sub(adjustment));
        wb = 0;
    }
    if wc < 0 {
        let adjustment = wc.wrapping_mul(wb).checked_div(wa.wrapping_add(wb))?;
        wb = wb.wrapping_add(adjustment);
        wa = wa.wrapping_add(wc.wrapping_sub(adjustment));
        wc = 0;
    }
    if wa < 0 {
        let adjustment = wa.wrapping_mul(wb).checked_div(wc.wrapping_add(wb))?;
        wc = wc.wrapping_add(wa.wrapping_sub(adjustment));
        wb = wb.wrapping_add(adjustment);
        wa = 0;
    }
    let mut color = std::array::from_fn(|channel| {
        ((i32::from(colors[1][channel])
            .wrapping_mul(wb)
            .wrapping_add(i32::from(colors[0][channel]).wrapping_mul(wa))
            .wrapping_add(i32::from(colors[2][channel]).wrapping_mul(wc)))
            >> 8) as u8
    });
    for channel in 0..3 {
        color[channel] = (u16::from(color[channel]) * 2
            + if flags & 2 != 0 {
                u16::from(ambient[channel])
            } else {
                0
            })
        .min(255) as u8;
    }
    Some(color)
}

fn split_color(color: [u8; 4], minimum: u8) -> [[u8; 4]; 2] {
    let maximum = color[..3].iter().copied().max().unwrap_or(0).max(1);
    let diffuse = if maximum < minimum {
        scale_value(color, f64::from(minimum) / f64::from(maximum))
    } else {
        color
    };
    let mut ambient = color;
    if maximum > 96 {
        let factor = (f64::from(96.0_f32 * 255.0 / f32::from(maximum)) - f64::from(0.5_f32))
            .round_ties_even() as u32;
        for channel in &mut ambient[..3] {
            *channel = ((u32::from(*channel) * factor + 255) >> 8) as u8;
        }
    }
    [diffuse, ambient]
}

fn scale_value(color: [u8; 4], factor: f64) -> [u8; 4] {
    let rgb = [color[2], color[1], color[0]]
        .map(|byte| f64::from(f32::from(byte) * f32::from_bits(0x3b80_8081)));
    let largest = if rgb[0] > rgb[1] {
        if rgb[0] > rgb[2] { 0 } else { 2 }
    } else if rgb[1] > rgb[2] {
        1
    } else {
        2
    };
    let value = rgb[largest];
    let delta = value - rgb[0].min(rgb[1]).min(rgb[2]);
    let saturation = if value == 0.0 {
        0.0
    } else {
        (delta / value) as f32
    };
    let scaled = f64::from((value * factor) as f32);
    let converted = if saturation == 0.0 {
        [scaled; 3]
    } else {
        let hue = match largest {
            0 => (rgb[1] - rgb[2]) / delta,
            1 => (rgb[2] - rgb[0]) / delta + 2.0,
            _ => (rgb[0] - rgb[1]) / delta + 4.0,
        } as f32;
        let mut hue = hue * 60.0;
        if hue < 0.0 {
            hue += 360.0;
        }
        if hue >= 360.0 {
            hue -= 360.0;
        }
        let angle = hue * f32::from_bits(0x3c88_8889);
        let sector = ((f64::from(angle) - 0.5).round_ties_even() as i32).min(5);
        let fraction = f64::from(angle) - f64::from(sector);
        let saturation = f64::from(saturation.min(1.0));
        let low = (1.0 - saturation) * scaled;
        let descending = (1.0 - saturation * fraction) * scaled;
        let ascending = (1.0 - (1.0 - fraction) * saturation) * scaled;
        match sector {
            0 => [scaled, ascending, low],
            1 => [descending, scaled, low],
            2 => [low, scaled, ascending],
            3 => [low, descending, scaled],
            4 => [ascending, low, scaled],
            _ => [scaled, low, descending],
        }
    };
    let packed =
        converted.map(|value| (f64::from(value as f32) * 255.0).round_ties_even() as i32 as u8);
    [packed[2], packed[1], packed[0], 255]
}

#[cfg(test)]
#[path = "../../tests/stock_seed/world_model_floor_light_native.rs"]
mod tests;
