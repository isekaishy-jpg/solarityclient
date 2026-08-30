//! Placed WMO transforms and allocation-free repeated frustum selection.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};

use crate::{WorldCameraError, WorldFrustum};

use super::{WorldModelMeshPlan, WorldModelPlacementError};

/// One shared WMO mesh transformed by an owning MODF or game object.
pub struct PlacedWorldModelDrawPlan {
    mesh: Arc<WorldModelMeshPlan>,
    transform: Mat4,
    group_bounds: Vec<OrientedBounds>,
    draw_bounds: Vec<OrientedBounds>,
    draw_group_slots: Vec<usize>,
    visible_groups: Vec<bool>,
}

impl PlacedWorldModelDrawPlan {
    /// Creates a placement in the server/ECS Z-up world basis.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelPlacementError`] for a non-finite or non-positive
    /// transform, or a mesh whose draw/group ownership is inconsistent.
    pub fn prepare(
        mesh: Arc<WorldModelMeshPlan>,
        position: Vec3,
        rotation_degrees: Vec3,
        scale: f32,
    ) -> Result<Self, WorldModelPlacementError> {
        if !position.is_finite()
            || !rotation_degrees.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
        {
            return Err(WorldModelPlacementError::InvalidTransform {
                path: mesh.path().clone(),
            });
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
        if !transform.is_finite() || transform.determinant().abs() <= f32::EPSILON {
            return Err(WorldModelPlacementError::InvalidTransform {
                path: mesh.path().clone(),
            });
        }

        let group_bounds: Vec<OrientedBounds> = mesh
            .groups()
            .iter()
            .map(|group| OrientedBounds::prepare(group.bounds(), transform))
            .collect();
        let group_slots = mesh
            .groups()
            .iter()
            .enumerate()
            .map(|(slot, group)| (group.group_index(), slot))
            .collect::<HashMap<_, _>>();
        let mut draw_bounds = Vec::with_capacity(mesh.draws().len());
        let mut draw_group_slots = Vec::with_capacity(mesh.draws().len());
        for (draw_index, draw) in mesh.draws().iter().copied().enumerate() {
            let Some(group_slot) = group_slots.get(&draw.group_index()).copied() else {
                return Err(WorldModelPlacementError::MissingGroup {
                    path: mesh.path().clone(),
                    draw_index,
                    group_index: draw.group_index(),
                });
            };
            let bounds = draw.bounds().map(|point| point.map(f32::from));
            draw_bounds.push(OrientedBounds::prepare(bounds, transform));
            draw_group_slots.push(group_slot);
        }
        let visible_groups = vec![false; group_bounds.len()];
        Ok(Self {
            mesh,
            transform,
            group_bounds,
            draw_bounds,
            draw_group_slots,
            visible_groups,
        })
    }

    /// Returns the shared immutable WMO generation.
    #[must_use]
    pub const fn mesh(&self) -> &Arc<WorldModelMeshPlan> {
        &self.mesh
    }

    /// Returns the root-local to world-space owner transform.
    #[must_use]
    pub const fn transform(&self) -> Mat4 {
        self.transform
    }

    /// Replaces `output` with visible combined MOBA draw indices.
    ///
    /// Group rejection precedes batch tests. Both scratch buffers retain their
    /// allocations across frames, and the supplied output can do the same.
    ///
    /// # Errors
    ///
    /// Returns [`WorldCameraError`] if any prepared bound is non-finite.
    pub fn select_visible_draws(
        &mut self,
        frustum: WorldFrustum,
        output: &mut Vec<usize>,
    ) -> Result<(), WorldCameraError> {
        output.clear();
        for (visible, bounds) in self.visible_groups.iter_mut().zip(&self.group_bounds) {
            *visible = bounds.intersects(frustum)?;
        }
        for draw_index in 0..self.draw_bounds.len() {
            if self.visible_groups[self.draw_group_slots[draw_index]]
                && self.draw_bounds[draw_index].intersects(frustum)?
            {
                output.push(draw_index);
            }
        }
        Ok(())
    }
}

struct OrientedBounds {
    center: Vec3,
    axes: [Vec3; 3],
}

impl OrientedBounds {
    fn prepare(bounds: [[f32; 3]; 2], transform: Mat4) -> Self {
        let minimum = Vec3::from_array(bounds[0]);
        let maximum = Vec3::from_array(bounds[1]);
        let center = (minimum + maximum) * 0.5;
        let half = (maximum - minimum) * 0.5;
        Self {
            center: transform.transform_point3(center),
            axes: [
                transform.transform_vector3(Vec3::X * half.x),
                transform.transform_vector3(Vec3::Y * half.y),
                transform.transform_vector3(Vec3::Z * half.z),
            ],
        }
    }

    fn intersects(&self, frustum: WorldFrustum) -> Result<bool, WorldCameraError> {
        frustum.intersects_box(self.center, self.axes[0], self.axes[1], self.axes[2])
    }
}
