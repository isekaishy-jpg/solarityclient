//! Placed build-12340 WMO generations shared by camera and movement queries.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use solarity_asset::{DecodedWorldModel, DecodedWorldModelGroup};
use thiserror::Error;

use super::movement_collection::{MovementBspQuery, cached_leaf_eligibility};

const COLLISION_TOLERANCE: f32 = 0.0001;
const DETERMINANT_TOLERANCE: f32 = 0.000_001;

/// Invalid WMO placement or camera-ray input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldModelCollisionError {
    /// A selected BSP has a cycle, invalid axis, or invalid child.
    #[error("world-model collision BSP is invalid")]
    InvalidBsp,
    /// The query names a group outside the complete admitted WMO generation.
    #[error("world-model collision group {group_index} is outside the admitted model")]
    InvalidGroup {
        /// Invalid root group index.
        group_index: usize,
    },
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
    pub(super) model: Arc<DecodedWorldModel>,
    pub(super) transform: Mat4,
    pub(super) inverse_transform: Mat4,
    pub(super) root_bounds: [Vec3; 2],
    pub(super) liquid_ray_meshes: Vec<Option<super::world_model_water_ray::LiquidRayMesh>>,
    group_bounds: Vec<[Vec3; 2]>,
    camera_bounds: Option<[Vec3; 2]>,
    movement_group_bounds: Vec<[Vec3; 2]>,
    placement_bounds_scratch: Vec<[Vec3; 2]>,
    pub(super) movement_pending: Vec<MovementBspQuery>,
    pub(super) movement_faces: Vec<bool>,
    pub(super) movement_cached_leaves: Vec<Vec<bool>>,
    pub(super) floor_probe: super::world_model_floor::FloorProbeScratch,
}

impl PlacedWorldModelCollision {
    /// Runs the solid 77F310 WMO branch over native MOGI group admission.
    /// Camera mask 0x100171 excludes MOPY 0x82 and includes the distance limit.
    /// The retained native BSP scratch is shared with floor
    /// registration, with separate face admission and result channels.
    ///
    /// # Errors
    /// Rejects invalid endpoints, fractions, or selected BSP geometry.
    pub fn trace_solid_camera(
        &mut self,
        start: Vec3,
        end: Vec3,
        maximum: f32,
    ) -> Result<Option<f32>, WorldModelCollisionError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        if !maximum.is_finite() || !(0.0..=1.0).contains(&maximum) {
            return Err(WorldModelCollisionError::InvalidMaximumFraction);
        }
        let start = super::movement_collection::transform_point(self.inverse_transform, start);
        let end = super::movement_collection::transform_point(self.inverse_transform, end);
        let mut nearest = maximum;
        let mut hit = false;
        for index in 0..self.model.groups().len() {
            let bounds = self.model.group_info()[index]
                .bounds()
                .map(Vec3::from_array);
            if super::world_model_water_ray::clipped_segment(start, end, bounds).is_none() {
                continue;
            }
            if let Some(contact) =
                self.probe_scene_camera_group_floor(index, start, end, nearest)?
            {
                nearest = contact.fraction();
                hit = true;
            }
        }
        Ok(hit.then_some(nearest))
    }

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
        Self::prepare_transform(model, transform)
    }

    /// Creates a placement from an authoritative local-to-world transform.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError::InvalidPlacement`] for non-finite,
    /// singular transforms or non-finite transformed group bounds.
    pub fn prepare_transform(
        model: Arc<DecodedWorldModel>,
        transform: Mat4,
    ) -> Result<Self, WorldModelCollisionError> {
        if !transform.is_finite() || transform.determinant().abs() <= f32::EPSILON {
            return Err(WorldModelCollisionError::InvalidPlacement);
        }
        let inverse_transform = transform.inverse();
        Self::prepare_transforms(model, transform, inverse_transform)
    }

    /// Retains both matrices resolved by a stock placement or transport owner.
    ///
    /// Stock stores the two float images independently. Re-inverting one image
    /// can move a transformed query onto the other side of a BSP boundary.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError::InvalidPlacement`] for non-finite
    /// or singular matrices or non-finite transformed group bounds.
    pub fn prepare_transforms(
        model: Arc<DecodedWorldModel>,
        transform: Mat4,
        inverse_transform: Mat4,
    ) -> Result<Self, WorldModelCollisionError> {
        if !transform.is_finite()
            || !inverse_transform.is_finite()
            || transform.determinant().abs() <= f32::EPSILON
            || inverse_transform.determinant().abs() <= f32::EPSILON
        {
            return Err(WorldModelCollisionError::InvalidPlacement);
        }
        let root_bounds = transformed_bounds(model.bounds(), transform)?;
        // 0x007BDE50 / 0x007AE720 transform root MOGI boxes for the placed
        // group-reference list, independently of the group's MOGP BSP region.
        let movement_group_bounds = model
            .group_info()
            .iter()
            .map(|group| transformed_bounds(group.bounds(), transform))
            .collect::<Result<Vec<_>, _>>()?;
        let group_bounds = model
            .groups()
            .iter()
            .map(|group| transformed_bounds(group.bounds(), transform))
            .collect::<Result<Vec<_>, _>>()?;
        let movement_cached_leaves = model.groups().iter().map(cached_leaf_eligibility).collect();
        let camera_bounds = union_bounds(&group_bounds);
        let liquid_ray_meshes = model
            .groups()
            .iter()
            .map(|group| {
                group
                    .liquid()
                    .map(super::world_model_water_ray::LiquidRayMesh::new)
            })
            .collect();
        Ok(Self {
            liquid_ray_meshes,
            model,
            transform,
            inverse_transform,
            root_bounds,
            group_bounds,
            camera_bounds,
            movement_group_bounds,
            placement_bounds_scratch: Vec::new(),
            movement_pending: Vec::new(),
            movement_faces: Vec::new(),
            movement_cached_leaves,
            floor_probe: super::world_model_floor::FloorProbeScratch::default(),
        })
    }

    /// Updates a retained moving root without rebuilding its local BSP caches.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError::InvalidPlacement`] for invalid
    /// transforms or bounds. Failed updates preserve the queryable placement.
    pub fn set_transform(&mut self, transform: Mat4) -> Result<(), WorldModelCollisionError> {
        if transform == self.transform {
            return Ok(());
        }
        if !transform.is_finite() || transform.determinant().abs() <= f32::EPSILON {
            return Err(WorldModelCollisionError::InvalidPlacement);
        }
        self.set_transforms(transform, transform.inverse())
    }

    /// Updates both independently supplied native placement matrices.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError::InvalidPlacement`] for invalid
    /// transforms or bounds. Failed updates preserve the queryable placement.
    pub fn set_transforms(
        &mut self,
        transform: Mat4,
        inverse_transform: Mat4,
    ) -> Result<(), WorldModelCollisionError> {
        if !transform.is_finite()
            || !inverse_transform.is_finite()
            || transform.determinant().abs() <= f32::EPSILON
            || inverse_transform.determinant().abs() <= f32::EPSILON
        {
            return Err(WorldModelCollisionError::InvalidPlacement);
        }
        let root_bounds = transformed_bounds(self.model.bounds(), transform)?;
        self.placement_bounds_scratch.clear();
        for bounds in self
            .model
            .group_info()
            .iter()
            .map(|group| group.bounds())
            .chain(self.model.groups().iter().map(|group| group.bounds()))
        {
            self.placement_bounds_scratch
                .push(transformed_bounds(bounds, transform)?);
        }
        let groups = self.model.groups().len();
        self.movement_group_bounds
            .copy_from_slice(&self.placement_bounds_scratch[..groups]);
        self.group_bounds
            .copy_from_slice(&self.placement_bounds_scratch[groups..]);
        self.camera_bounds = union_bounds(&self.group_bounds);
        self.root_bounds = root_bounds;
        self.transform = transform;
        self.inverse_transform = inverse_transform;
        Ok(())
    }

    /// Returns the canonical shared WMO root path.
    #[must_use]
    pub fn model(&self) -> &Arc<DecodedWorldModel> {
        &self.model
    }

    /// Returns 7AE720's placed MOGI bounds used by scene and movement lists.
    ///
    /// # Errors
    /// Rejects a group outside this admitted root generation.
    pub fn scene_group_bounds(&self, group: usize) -> Result<[Vec3; 2], WorldModelCollisionError> {
        self.movement_group_bounds
            .get(group)
            .copied()
            .ok_or(WorldModelCollisionError::InvalidGroup { group_index: group })
    }

    /// Tests the placed root box used before ordinary movement collection.
    #[must_use]
    pub fn movement_intersects(&self, bounds: super::MovementCollisionBounds) -> bool {
        bounds_intersect([bounds.minimum(), bounds.maximum()], self.root_bounds)
    }

    /// Tests a placed group's box before visiting its registered M2 references.
    #[must_use]
    pub fn movement_group_intersects(
        &self,
        group: usize,
        bounds: super::MovementCollisionBounds,
    ) -> bool {
        self.movement_group_bounds
            .get(group)
            .is_some_and(|group_bounds| {
                bounds_intersect([bounds.minimum(), bounds.maximum()], *group_bounds)
            })
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

    /// Borrows a registered owner and its retained movement traversal scratch.
    pub fn instance_mut(&mut self, index: usize) -> Option<&mut PlacedWorldModelCollision> {
        self.instances.get_mut(index)
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
            // MOGP group boxes define this union. The independently authored
            // MOHD root box is not a substitute for camera group admission.
            if instance
                .camera_bounds
                .is_none_or(|bounds| !bounds_intersect(query_bounds, bounds))
            {
                continue;
            }
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
    segment_triangle_fraction(start, direction, first, second, third, maximum_fraction)
}

pub(super) fn segment_triangle_fraction(
    start: Vec3,
    direction: Vec3,
    first: Vec3,
    second: Vec3,
    third: Vec3,
    maximum_fraction: f32,
) -> Option<f32> {
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
    let bounds = super::MovementCollisionBounds::new(
        Vec3::from_array(bounds[0]),
        Vec3::from_array(bounds[1]),
    )
    .and_then(|bounds| bounds.transformed(transform))
    .map_err(|_| WorldModelCollisionError::InvalidPlacement)?;
    Ok([bounds.minimum(), bounds.maximum()])
}

pub(super) fn bounds_intersect(left: [Vec3; 2], right: [Vec3; 2]) -> bool {
    (0..3).all(|axis| {
        left[0][axis] <= right[1][axis] + COLLISION_TOLERANCE
            && right[0][axis] <= left[1][axis] + COLLISION_TOLERANCE
    })
}

fn union_bounds(bounds: &[[Vec3; 2]]) -> Option<[Vec3; 2]> {
    bounds
        .iter()
        .copied()
        .reduce(|[minimum, maximum], [next_minimum, next_maximum]| {
            [minimum.min(next_minimum), maximum.max(next_maximum)]
        })
}
