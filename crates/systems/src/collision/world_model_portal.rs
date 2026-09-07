//! Stock group-registration portal crossings (`0x007AF520`).

use glam::Vec3;
use solarity_asset::DecodedWorldModel;

use super::WorldModelCollisionError;

/// The nearest portal and the two groups selected by its signed MOPR side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelPortalHit {
    pub(super) fraction: f32,
    pub(super) source_group: usize,
    pub(super) destination_group: usize,
}

impl WorldModelPortalHit {
    /// Returns the crossing fraction along the supplied local-space segment.
    #[must_use]
    pub const fn fraction(self) -> f32 {
        self.fraction
    }

    /// Returns the group on the start point's side of the authored portal plane.
    #[must_use]
    pub const fn source_group(self) -> usize {
        self.source_group
    }

    /// Returns the group on the opposite side of the authored portal plane.
    #[must_use]
    pub const fn destination_group(self) -> usize {
        self.destination_group
    }
}

/// Probes an admitted group's portals for native spatial registration.
///
/// Endpoints are WMO-local. The immutable model contains every loaded group;
/// callers must resolve residency before reaching this boundary. Plane proximity
/// uses stock's 0.1 tolerance, polygon edges keep native inclusion rules, and
/// equal-distance portals replace the earlier MOPR result. Maximum fractions
/// above one are supported because stock's registration probe starts at 1.05.
///
/// # Errors
/// Returns [`WorldModelCollisionError`] for invalid endpoints, maximum fraction,
/// or a group index outside this admitted model.
pub fn probe_world_model_portals(
    model: &DecodedWorldModel,
    group_index: usize,
    start: Vec3,
    end: Vec3,
    maximum_fraction: f32,
) -> Result<Option<WorldModelPortalHit>, WorldModelCollisionError> {
    if !start.is_finite() || !end.is_finite() {
        return Err(WorldModelCollisionError::NonFiniteSegment);
    }
    if !maximum_fraction.is_finite() || maximum_fraction < 0.0 {
        return Err(WorldModelCollisionError::InvalidMaximumFraction);
    }
    let group = model
        .groups()
        .get(group_index)
        .ok_or(WorldModelCollisionError::InvalidGroup { group_index })?;
    // AF550 keeps endpoint subtraction and normalization on the x87 stack.
    let delta = end.as_dvec3() - start.as_dvec3();
    let length = ((delta.z * delta.z + delta.x * delta.x) + delta.y * delta.y).sqrt();
    if length == 0.0 {
        return Ok(None);
    }
    let inverse = length.recip();
    let direction = (delta * inverse).as_vec3();
    let inverse = inverse as f32;
    let mut nearest = (length * f64::from(maximum_fraction)) as f32;
    let mut hit = None;
    let first = usize::from(group.portal_reference_start());
    let count = usize::from(group.portal_reference_count());
    for reference in &model.portal_references()[first..first + count] {
        let portal = model.portals()[usize::from(reference.portal_index())];
        let normal = Vec3::from_array(portal.normal());
        let Some((distance, point)) = plane_hit(start, direction, normal, portal.distance()) else {
            continue;
        };
        if distance < 0.0 || distance > nearest {
            continue;
        }
        let first = usize::from(portal.vertex_start());
        let count = usize::from(portal.vertex_count());
        if !polygon_contains(
            point,
            &model.portal_vertices()[first..first + count],
            normal,
        ) {
            continue;
        }
        nearest = distance;
        let normal = normal.as_dvec3();
        let start = start.as_dvec3();
        let signed = ((normal.z * start.z + normal.y * start.y) + start.x * normal.x)
            + f64::from(portal.distance());
        let adjacent = usize::from(reference.group_index());
        let (source_group, destination_group) = if (signed < 0.0) == (reference.side() > 0) {
            (adjacent, group_index)
        } else {
            (group_index, adjacent)
        };
        hit = Some(WorldModelPortalHit {
            fraction: (f64::from(nearest) * f64::from(inverse)) as f32,
            source_group,
            destination_group,
        });
    }
    Ok(hit)
}

pub(super) fn plane_hit(
    start: Vec3,
    direction: Vec3,
    normal: Vec3,
    distance: f32,
) -> Option<(f32, Vec3)> {
    let start = start.as_dvec3();
    let direction = direction.as_dvec3();
    let normal = normal.as_dvec3();
    let dot = (direction.y * normal.y + direction.z * normal.z) + normal.x * direction.x;
    let tolerance = f64::from(0.1_f32);
    if dot.abs() < 0.0001_f64 {
        let signed =
            ((start.y * normal.y + start.z * normal.z) + start.x * normal.x) + f64::from(distance);
        return (signed.abs() < tolerance).then_some((0.0, start.as_vec3()));
    }
    let signed =
        ((start.x * normal.x + normal.z * start.z) + normal.y * start.y) + f64::from(distance);
    let distance = if signed.abs() < tolerance {
        0.0
    } else {
        -(signed / dot)
    };
    // 982FB0 stores the scalar result but keeps its extended value for the point.
    Some((distance as f32, (start + direction * distance).as_vec3()))
}

pub(super) fn polygon_contains(point: Vec3, vertices: &[[f32; 3]], normal: Vec3) -> bool {
    let Some(&last) = vertices.last() else {
        return false;
    };
    let normal = normal.abs();
    let axis = if normal.x <= normal.y {
        if normal.z < normal.y { 1 } else { 2 }
    } else if normal.z < normal.x {
        0
    } else {
        2
    };
    let (x, y) = [(1, 2), (2, 0), (0, 1)][axis];
    let point = point.as_dvec3();
    let mut previous = Vec3::from_array(last).as_dvec3();
    let mut above = point[y] <= previous[y];
    let mut inside = false;
    for vertex in vertices {
        let current = Vec3::from_array(*vertex).as_dvec3();
        let next_above = point[y] <= current[y];
        if above != next_above {
            let first = (current[y] - point[y]) * (previous[x] - current[x]);
            let second = (previous[y] - current[y]) * (current[x] - point[x]);
            if (second <= first) == next_above {
                inside = !inside;
            }
        }
        previous = current;
        above = next_above;
    }
    inside
}
