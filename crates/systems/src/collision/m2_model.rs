//! Reusable camera collision for placed build-12340 M2 collision meshes.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use solarity_asset::DecodedM2Model;
use thiserror::Error;

use super::world_model::{
    bounds_intersect, placement_transform, segment_triangle_fraction, transformed_bounds,
};

/// Invalid M2 placement or camera-ray input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2CollisionError {
    /// Placement position, rotation, or scale is invalid.
    #[error("M2 collision placement is invalid")]
    InvalidPlacement,
    /// A segment endpoint is NaN or infinite.
    #[error("M2 collision segment is not finite")]
    NonFiniteSegment,
    /// Maximum fraction is negative, NaN, or infinite.
    #[error("M2 collision maximum fraction is invalid")]
    InvalidMaximumFraction,
}

/// One shared M2 generation transformed by an owning MDDF placement.
pub struct PlacedM2Collision {
    model: Arc<DecodedM2Model>,
    inverse_transform: Mat4,
    collision_bounds: Option<[Vec3; 2]>,
}

impl PlacedM2Collision {
    /// Creates a placement in the server/ECS Z-up world basis.
    ///
    /// Models without dedicated collision arrays remain valid scene owners but
    /// cannot obstruct the camera. Render geometry is never substituted.
    ///
    /// # Errors
    ///
    /// Returns [`M2CollisionError::InvalidPlacement`] when transform inputs are
    /// non-finite, scale is not positive, or inversion fails.
    pub fn prepare(
        model: Arc<DecodedM2Model>,
        position: Vec3,
        rotation_degrees: Vec3,
        scale: f32,
    ) -> Result<Self, M2CollisionError> {
        let transform = placement_transform(position, rotation_degrees, scale)
            .map_err(|_| M2CollisionError::InvalidPlacement)?;
        let inverse_transform = transform.inverse();
        if !inverse_transform.is_finite() {
            return Err(M2CollisionError::InvalidPlacement);
        }
        let collision_bounds = model
            .collision_mesh()
            .map(|mesh| {
                let bounds = mesh.bounds();
                transformed_bounds(
                    [bounds.minimum().to_array(), bounds.maximum().to_array()],
                    transform,
                )
            })
            .transpose()
            .map_err(|_| M2CollisionError::InvalidPlacement)?;
        Ok(Self {
            model,
            inverse_transform,
            collision_bounds,
        })
    }

    /// Returns the shared decoded M2 generation.
    #[must_use]
    pub fn model(&self) -> &Arc<DecodedM2Model> {
        &self.model
    }
}

/// Main-thread placed-M2 scene with allocation-free repeated traces.
#[derive(Default)]
pub struct M2CollisionScene {
    instances: Vec<PlacedM2Collision>,
}

impl M2CollisionScene {
    /// Creates an empty placed-M2 collision owner.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            instances: Vec::new(),
        }
    }

    /// Adds one already validated placed M2 generation.
    pub fn add(&mut self, placement: PlacedM2Collision) {
        self.instances.push(placement);
    }

    /// Returns the number of independently transformed M2 owners.
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Traces dedicated unanimated collision triangles and returns the nearest fraction.
    ///
    /// # Errors
    ///
    /// Returns [`M2CollisionError`] for invalid endpoints or maximum.
    pub fn trace_camera(
        &self,
        start: Vec3,
        end: Vec3,
        maximum_fraction: f32,
    ) -> Result<Option<f32>, M2CollisionError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(M2CollisionError::NonFiniteSegment);
        }
        if !maximum_fraction.is_finite() || maximum_fraction < 0.0 {
            return Err(M2CollisionError::InvalidMaximumFraction);
        }
        let mut nearest = maximum_fraction.min(1.0);
        if nearest <= 0.0 || (end - start).length_squared() < 1.0e-12 {
            return Ok(None);
        }
        let limited_end = start + (end - start) * nearest;
        let query_bounds = [start.min(limited_end), start.max(limited_end)];
        let mut found = false;
        for instance in &self.instances {
            let Some(collision_bounds) = instance.collision_bounds else {
                continue;
            };
            if !bounds_intersect(query_bounds, collision_bounds) {
                continue;
            }
            let Some(mesh) = instance.model.collision_mesh() else {
                continue;
            };
            let local_start = instance.inverse_transform.transform_point3(start);
            let local_end = instance.inverse_transform.transform_point3(end);
            let direction = local_end - local_start;
            for triangle in mesh.indices().as_chunks::<3>().0 {
                let first = mesh.vertices()[usize::from(triangle[0])];
                let second = mesh.vertices()[usize::from(triangle[1])];
                let third = mesh.vertices()[usize::from(triangle[2])];
                if let Some(hit) =
                    segment_triangle_fraction(local_start, direction, first, second, third, nearest)
                {
                    nearest = hit;
                    found = true;
                }
            }
        }
        Ok(found.then_some(nearest))
    }
}
