//! Camera MFOG ownership and distance to an exterior portal (7A1150/7D77C0).

use glam::Vec3;
use solarity_asset::{DecodedWorldModel, WorldModelFogPalette, sample_world_model_fog};

use super::movement_collection::transform_point;
use super::world_model_portal::{plane_hit_with_tolerance, polygon_contains};
use super::{PlacedWorldModelCollision, WorldModelCollisionError};

/// Both camera-group fog banks and the nearest exterior boundary distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelFogEnvironment {
    palette: WorldModelFogPalette,
    boundary_distance: Option<f32>,
}

impl WorldModelFogEnvironment {
    /// Returns entry zero after the first camera group's local MFOG overlays.
    #[must_use]
    pub const fn palette(self) -> WorldModelFogPalette {
        self.palette
    }

    /// Returns no distance when all camera groups suppress indoor fog.
    /// `f32::MAX` denotes an interior with no exterior portal within 25 units.
    #[must_use]
    pub const fn boundary_distance(self) -> Option<f32> {
        self.boundary_distance
    }
}

impl PlacedWorldModelCollision {
    /// Resolves the selected camera groups' native MFOG environment.
    ///
    /// # Errors
    /// Returns invalid camera points or group indices in an admitted model.
    pub fn fog_environment(
        &self,
        group: usize,
        secondary_group: Option<usize>,
        world_position: Vec3,
    ) -> Result<Option<WorldModelFogEnvironment>, WorldModelCollisionError> {
        let position = transform_point(self.inverse_transform, world_position);
        if !position.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        let primary = self
            .model
            .groups()
            .get(group)
            .ok_or(WorldModelCollisionError::InvalidGroup { group_index: group })?;
        let mut boundary_distance: Option<f32> = None;
        for index in [Some(group), secondary_group].into_iter().flatten() {
            let group = self
                .model
                .groups()
                .get(index)
                .ok_or(WorldModelCollisionError::InvalidGroup { group_index: index })?;
            if group.flags() & 0x48 == 0 {
                let nearest = boundary_distance.get_or_insert(f32::MAX);
                exterior_distance(&self.model, index, None, 0, position, nearest);
            }
        }
        Ok(
            sample_world_model_fog(self.model.fogs(), primary.fog_ids(), position).map(|palette| {
                WorldModelFogEnvironment {
                    palette,
                    boundary_distance,
                }
            }),
        )
    }
}

fn exterior_distance(
    model: &DecodedWorldModel,
    group: usize,
    parent: Option<usize>,
    depth: usize,
    position: Vec3,
    nearest: &mut f32,
) {
    if depth >= 4 {
        return;
    }
    let selected = &model.groups()[group];
    let first = usize::from(selected.portal_reference_start());
    let count = usize::from(selected.portal_reference_count());
    for reference in &model.portal_references()[first..first + count] {
        let adjacent = usize::from(reference.group_index());
        if Some(adjacent) == parent {
            continue;
        }
        if model.group_info()[adjacent].flags() & 0x48 == 0 {
            exterior_distance(model, adjacent, Some(group), depth + 1, position, nearest);
        } else {
            let portal = model.portals()[usize::from(reference.portal_index())];
            let first = usize::from(portal.vertex_start());
            let count = usize::from(portal.vertex_count());
            let distance = polygon_distance(
                position,
                &model.portal_vertices()[first..first + count],
                Vec3::from_array(portal.normal()),
                portal.distance(),
            );
            if distance < 25. && distance < f64::from(*nearest) {
                *nearest = distance as f32;
            }
        }
    }
}

/// 984E50 preserves unnormalized planes and falls back to the nearest edge.
fn polygon_distance(point: Vec3, vertices: &[[f32; 3]], normal: Vec3, distance: f32) -> f64 {
    let p = point.as_dvec3();
    let n = normal.as_dvec3();
    let signed = ((p.x * n.x + p.y * n.y) + p.z * n.z) + f64::from(distance);
    let projected = if signed.abs() > 0.001 {
        // Both the endpoint and its subtraction from the origin are stored
        // floats in 985200, before 982FB0 keeps its intersection on x87.
        let endpoint = if signed > 0. {
            point - normal
        } else {
            point + normal
        };
        plane_hit_with_tolerance(point, endpoint - point, normal, distance, 0.01)
            .map_or(point, |(_, projected)| projected)
    } else {
        point
    };
    if polygon_contains(projected, vertices, normal) {
        return signed.abs();
    }
    let mut minimum = f32::MAX;
    let Some(last) = vertices.last() else {
        return f64::from(minimum);
    };
    let mut previous = Vec3::from_array(*last).as_dvec3();
    for vertex in vertices {
        let current = Vec3::from_array(*vertex).as_dvec3();
        let edge = current - previous;
        let mut delta = p - previous;
        let projection = (edge.y * delta.y + edge.z * delta.z) + edge.x * delta.x;
        if projection > 0. {
            let length = (edge.y * edge.y + edge.z * edge.z) + edge.x * edge.x;
            let projection = f64::from(projection as f32);
            let scale = if projection <= length {
                projection / f64::from(length as f32)
            } else {
                1.
            };
            delta -= edge * scale;
        }
        let distance = ((delta.x * delta.x + delta.y * delta.y) + delta.z * delta.z).sqrt();
        if distance < f64::from(minimum) {
            minimum = distance as f32;
        }
        previous = current;
    }
    f64::from(minimum)
}

#[cfg(test)]
#[path = "../../tests/stock_seed/world_model_fog_distance_native.rs"]
mod tests;
