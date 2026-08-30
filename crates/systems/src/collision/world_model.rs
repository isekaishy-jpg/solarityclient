//! Reusable BSP camera collision for placed build-12340 WMO generations.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use solarity_asset::{DecodedWorldModel, DecodedWorldModelGroup};
use thiserror::Error;

const COLLISION_TOLERANCE: f32 = 0.0001;
const DETERMINANT_TOLERANCE: f32 = 0.000_001;

/// Invalid WMO placement or camera-ray input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldModelCollisionError {
    /// Placement position, rotation, or scale is invalid.
    #[error("world-model collision placement is invalid")]
    InvalidPlacement,
    /// A segment endpoint is NaN or infinite.
    #[error("world-model collision segment is not finite")]
    NonFiniteSegment,
    /// Maximum fraction is negative, NaN, or infinite.
    #[error("world-model collision maximum fraction is invalid")]
    InvalidMaximumFraction,
}

/// One shared WMO generation transformed by an owning MODF or game object.
pub struct PlacedWorldModelCollision {
    model: Arc<DecodedWorldModel>,
    inverse_transform: Mat4,
    group_bounds: Vec<[Vec3; 2]>,
}

impl PlacedWorldModelCollision {
    /// Creates a placement in the server/ECS Z-up world basis.
    ///
    /// Static MODF callers pass unit scale. Replicated game-object WMOs may
    /// pass their authoritative positive scale through the same stock transform.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelCollisionError::InvalidPlacement`] when transform
    /// inputs are non-finite, scale is not positive, or inversion fails.
    pub fn prepare(
        model: Arc<DecodedWorldModel>,
        position: Vec3,
        rotation_degrees: Vec3,
        scale: f32,
    ) -> Result<Self, WorldModelCollisionError> {
        if !position.is_finite()
            || !rotation_degrees.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
        {
            return Err(WorldModelCollisionError::InvalidPlacement);
        }
        let transform = placement_transform(position, rotation_degrees, scale)?;
        let inverse_transform = transform.inverse();
        if !inverse_transform.is_finite() {
            return Err(WorldModelCollisionError::InvalidPlacement);
        }
        let group_bounds = model
            .groups()
            .iter()
            .map(|group| transformed_bounds(group.bounds(), transform))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            model,
            inverse_transform,
            group_bounds,
        })
    }

    /// Returns the canonical shared WMO root path.
    #[must_use]
    pub fn model(&self) -> &Arc<DecodedWorldModel> {
        &self.model
    }
}

/// Main-thread static/dynamic WMO scene with allocation-free repeated traces.
#[derive(Default)]
pub struct WorldModelCollisionScene {
    instances: Vec<PlacedWorldModelCollision>,
    pending_nodes: Vec<usize>,
    visited_nodes: Vec<u32>,
    visited_faces: Vec<u32>,
    generation: u32,
}

impl WorldModelCollisionScene {
    /// Creates an empty placed-WMO collision owner.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            instances: Vec::new(),
            pending_nodes: Vec::new(),
            visited_nodes: Vec::new(),
            visited_faces: Vec::new(),
            generation: 0,
        }
    }

    /// Adds one already validated placed WMO generation.
    pub fn add(&mut self, placement: PlacedWorldModelCollision) {
        self.instances.push(placement);
    }

    /// Returns the number of independently transformed WMO owners.
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Traces stock camera-collidable MOPY faces and returns the nearest fraction.
    ///
    /// MOBN traversal storage and generation stamps are retained across the
    /// camera's nine obstruction probes, avoiding per-probe allocation.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelCollisionError`] for invalid endpoints or maximum.
    pub fn trace_camera(
        &mut self,
        start: Vec3,
        end: Vec3,
        maximum_fraction: f32,
    ) -> Result<Option<f32>, WorldModelCollisionError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        if !maximum_fraction.is_finite() || maximum_fraction < 0.0 {
            return Err(WorldModelCollisionError::InvalidMaximumFraction);
        }
        let mut nearest = maximum_fraction.min(1.0);
        if nearest <= 0.0 || (end - start).length_squared() < 1.0e-12 {
            return Ok(None);
        }
        let limited_end = start + (end - start) * nearest;
        let query_bounds = [start.min(limited_end), start.max(limited_end)];
        let mut found = false;
        for instance_index in 0..self.instances.len() {
            let instance = &self.instances[instance_index];
            let local_start = instance.inverse_transform.transform_point3(start);
            let local_end = instance.inverse_transform.transform_point3(end);
            for group_index in 0..instance.model.groups().len() {
                if !bounds_intersect(query_bounds, instance.group_bounds[group_index]) {
                    continue;
                }
                let group = &instance.model.groups()[group_index];
                if let Some(hit) = trace_group_camera(
                    group,
                    local_start,
                    local_end,
                    nearest,
                    &mut self.pending_nodes,
                    &mut self.visited_nodes,
                    &mut self.visited_faces,
                    &mut self.generation,
                ) {
                    nearest = hit;
                    found = true;
                }
            }
        }
        Ok(found.then_some(nearest))
    }
}

#[allow(clippy::too_many_arguments)]
fn trace_group_camera(
    group: &DecodedWorldModelGroup,
    start: Vec3,
    end: Vec3,
    maximum_fraction: f32,
    pending_nodes: &mut Vec<usize>,
    visited_nodes: &mut Vec<u32>,
    visited_faces: &mut Vec<u32>,
    generation: &mut u32,
) -> Option<f32> {
    if group.bsp_nodes().is_empty() || group.bsp_faces().is_empty() {
        return None;
    }
    *generation = generation.wrapping_add(1);
    if *generation == 0 {
        visited_nodes.fill(0);
        visited_faces.fill(0);
        *generation = 1;
    }
    visited_nodes.resize(visited_nodes.len().max(group.bsp_nodes().len()), 0);
    visited_faces.resize(visited_faces.len().max(group.polygons().len()), 0);
    pending_nodes.clear();
    pending_nodes.push(0);
    let direction = end - start;
    let mut nearest = maximum_fraction;
    let mut found = false;
    while let Some(node_index) = pending_nodes.pop() {
        if node_index >= group.bsp_nodes().len() || visited_nodes[node_index] == *generation {
            continue;
        }
        visited_nodes[node_index] = *generation;
        let node = group.bsp_nodes()[node_index];
        let first_face = node.first_face() as usize;
        let face_end = first_face + usize::from(node.face_count());
        for reference in &group.bsp_faces()[first_face..face_end] {
            let face = usize::from(*reference);
            if visited_faces[face] == *generation {
                continue;
            }
            visited_faces[face] = *generation;
            if !group.polygons()[face].is_camera_collidable() {
                continue;
            }
            if let Some(hit) = triangle_fraction(group, face, start, direction, nearest) {
                nearest = hit;
                found = true;
            }
        }

        let plane_type = node.flags() & 0x7;
        if plane_type == 4 || plane_type > 2 {
            continue;
        }
        let axis = usize::from(plane_type);
        let start_distance = start[axis] - node.plane_distance();
        let end_distance = end[axis] - node.plane_distance();
        if (start_distance <= COLLISION_TOLERANCE || end_distance <= COLLISION_TOLERANCE)
            && let Ok(child) = usize::try_from(node.negative_child())
        {
            pending_nodes.push(child);
        }
        if (start_distance >= -COLLISION_TOLERANCE || end_distance >= -COLLISION_TOLERANCE)
            && let Ok(child) = usize::try_from(node.positive_child())
        {
            pending_nodes.push(child);
        }
    }
    found.then_some(nearest)
}

fn triangle_fraction(
    group: &DecodedWorldModelGroup,
    face: usize,
    start: Vec3,
    direction: Vec3,
    maximum_fraction: f32,
) -> Option<f32> {
    let first_index = face * 3;
    let first = Vec3::from_array(group.vertices()[usize::from(group.indices()[first_index])]);
    let second = Vec3::from_array(group.vertices()[usize::from(group.indices()[first_index + 1])]);
    let third = Vec3::from_array(group.vertices()[usize::from(group.indices()[first_index + 2])]);
    let first_edge = second - first;
    let second_edge = third - first;
    let perpendicular = direction.cross(second_edge);
    let determinant = first_edge.dot(perpendicular);
    if !determinant.is_finite() || determinant.abs() < DETERMINANT_TOLERANCE {
        return None;
    }
    let inverse = determinant.recip();
    let offset = start - first;
    let u = offset.dot(perpendicular) * inverse;
    if !u.is_finite() || u < -COLLISION_TOLERANCE || u > 1.0 + COLLISION_TOLERANCE {
        return None;
    }
    let cross = offset.cross(first_edge);
    let v = direction.dot(cross) * inverse;
    if !v.is_finite() || v < -COLLISION_TOLERANCE || u + v > 1.0 + COLLISION_TOLERANCE {
        return None;
    }
    let fraction = second_edge.dot(cross) * inverse;
    if !fraction.is_finite()
        || fraction < -COLLISION_TOLERANCE
        || fraction > maximum_fraction + COLLISION_TOLERANCE
    {
        return None;
    }
    Some(fraction.clamp(0.0, maximum_fraction))
}

pub(super) fn placement_transform(
    position: Vec3,
    rotation_degrees: Vec3,
    scale: f32,
) -> Result<Mat4, WorldModelCollisionError> {
    if !position.is_finite() || !rotation_degrees.is_finite() || !scale.is_finite() || scale <= 0.0
    {
        return Err(WorldModelCollisionError::InvalidPlacement);
    }
    let radians = Vec3::new(
        rotation_degrees.x.to_radians(),
        rotation_degrees.y.to_radians(),
        rotation_degrees.z.to_radians(),
    );
    let transform = Mat4::from_translation(position)
        * Mat4::from_rotation_z(radians.y + std::f32::consts::PI)
        * Mat4::from_rotation_y(radians.x)
        * Mat4::from_rotation_x(radians.z)
        * Mat4::from_scale(Vec3::splat(scale));
    let determinant = transform.determinant();
    if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
        return Err(WorldModelCollisionError::InvalidPlacement);
    }
    Ok(transform)
}

pub(super) fn transformed_bounds(
    bounds: [[f32; 3]; 2],
    transform: Mat4,
) -> Result<[Vec3; 2], WorldModelCollisionError> {
    let [minimum, maximum] = bounds.map(Vec3::from_array);
    let mut world_minimum = Vec3::splat(f32::INFINITY);
    let mut world_maximum = Vec3::splat(f32::NEG_INFINITY);
    for x in [minimum.x, maximum.x] {
        for y in [minimum.y, maximum.y] {
            for z in [minimum.z, maximum.z] {
                let point = transform.transform_point3(Vec3::new(x, y, z));
                world_minimum = world_minimum.min(point);
                world_maximum = world_maximum.max(point);
            }
        }
    }
    if !world_minimum.is_finite() || !world_maximum.is_finite() {
        return Err(WorldModelCollisionError::InvalidPlacement);
    }
    Ok([world_minimum, world_maximum])
}

fn bounds_intersect(left: [Vec3; 2], right: [Vec3; 2]) -> bool {
    (0..3).all(|axis| {
        left[0][axis] <= right[1][axis] + COLLISION_TOLERANCE
            && right[0][axis] <= left[1][axis] + COLLISION_TOLERANCE
    })
}
