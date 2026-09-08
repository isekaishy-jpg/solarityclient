//! Positive-child-first WMO face selection from `0x007CA920`.

use glam::Vec3;
use solarity_asset::DecodedWorldModelGroup;

use super::{
    MovementCollectionError, MovementCollisionBounds, transformed_query, world_model_triangle,
};
use crate::collision::{MovementCollisionTriangle, PlacedWorldModelCollision};

/// Resolved stock `bspcache` setting from `0x0078E400` / `0x0078DF90`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MovementBspCacheMode {
    /// Native default: eligible leaves use the cache's inclusive outcodes.
    #[default]
    Enabled,
    /// Explicitly disabled cache: all leaves use the uncached face-box test.
    Disabled,
}

/// Precomputes the native cache's 300-face / 450-distinct-vertex admission.
pub(in crate::collision) fn cached_leaf_eligibility(group: &DecodedWorldModelGroup) -> Vec<bool> {
    let mut vertices = std::collections::HashSet::with_capacity(450);
    group
        .bsp_nodes()
        .iter()
        .map(|node| {
            if node.flags() & 4 == 0 || node.face_count() > 300 {
                return false;
            }
            vertices.clear();
            let start = node.first_face() as usize;
            for &face in &group.bsp_faces()[start..start + usize::from(node.face_count())] {
                let first = usize::from(face) * 3;
                for &vertex in &group.indices()[first..first + 3] {
                    vertices.insert(vertex);
                    if vertices.len() > 450 {
                        return false;
                    }
                }
            }
            true
        })
        .collect()
}

/// One clipped native BSP recursion frame retained on the reusable work stack.
pub(in crate::collision) struct MovementBspQuery {
    node: usize,
    query: MovementCollisionBounds,
    region: MovementCollisionBounds,
    depth: usize,
}

impl PlacedWorldModelCollision {
    /// Collects 7CB180 camera-volume faces with the native `0x82` exclusion.
    ///
    /// Camera volume collection has no BSP leaf cache and does not substitute
    /// renderability or movement's `0x04` exclusion for `F_NOCAMCOLLIDE`.
    ///
    /// # Errors
    /// Rejects invalid transformed geometry or cyclic BSP traversal.
    pub fn append_camera_volume(
        &mut self,
        volume: &crate::PlayerCameraVolume,
        output: &mut Vec<MovementCollisionTriangle>,
    ) -> Result<(), MovementCollectionError> {
        let corners = volume
            .corners()
            .map(|point| super::transform_point(self.inverse_transform, point));
        let minimum = corners
            .iter()
            .copied()
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let maximum = corners
            .iter()
            .copied()
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        self.append_selected_faces(
            MovementCollisionBounds::new(minimum, maximum)?,
            MovementBspCacheMode::Disabled,
            0x82,
            output,
        )
    }

    /// Appends ordinary movement faces in root-group and native BSP order.
    ///
    /// MOPY `0x04` excludes movement; `0x02` only excludes the camera. The
    /// stock transient visited bit is kept in separate reusable storage here.
    /// The residency owner admits and deduplicates the complete placed model.
    /// `cache_mode` is the resolved `bspcache` CVar; its native default is enabled.
    ///
    /// # Errors
    /// Returns [`MovementCollectionError`] for invalid selected geometry, a
    /// cyclic source BSP, or stock's 8,192-face selection limit.
    pub fn append_movement(
        &mut self,
        bounds: MovementCollisionBounds,
        cache_mode: MovementBspCacheMode,
        output: &mut Vec<MovementCollisionTriangle>,
    ) -> Result<(), MovementCollectionError> {
        let local = transformed_query(bounds, self.inverse_transform)?;
        let root_bounds = self.model.bounds();
        if (0..3).any(|axis| {
            local.minimum[axis] > root_bounds[1][axis] || local.maximum[axis] < root_bounds[0][axis]
        }) {
            return Ok(());
        }
        self.append_selected_faces(local, cache_mode, 0x84, output)
    }

    fn append_selected_faces(
        &mut self,
        local: MovementCollisionBounds,
        cache_mode: MovementBspCacheMode,
        exclusions: u8,
        output: &mut Vec<MovementCollisionTriangle>,
    ) -> Result<(), MovementCollectionError> {
        for (group_index, group) in self.model.groups().iter().enumerate() {
            if group.flags() & 0x80 != 0 || group.bsp_nodes().is_empty() {
                continue;
            }
            let region = MovementCollisionBounds::new(
                Vec3::from_array(group.bounds()[0]),
                Vec3::from_array(group.bounds()[1]),
            )?;
            let selection = self.model.group_info()[group_index].bounds();
            if (0..3).any(|axis| {
                local.minimum[axis] > selection[1][axis] || local.maximum[axis] < selection[0][axis]
            }) {
                continue;
            }
            self.movement_faces.resize(group.polygons().len(), false);
            self.movement_faces.fill(false);
            self.movement_pending.clear();
            self.movement_pending.push(MovementBspQuery {
                node: 0,
                query: local,
                region,
                depth: 0,
            });
            let mut selected = 0;
            while let Some(MovementBspQuery {
                node: index,
                query,
                region,
                depth,
            }) = self.movement_pending.pop()
            {
                if depth >= group.bsp_nodes().len() {
                    return Err(MovementCollectionError::CyclicBsp);
                }
                let node = group.bsp_nodes()[index];
                if node.flags() & 4 != 0 {
                    let cached = cache_mode == MovementBspCacheMode::Enabled
                        && self.movement_cached_leaves[group_index][index];
                    let start = node.first_face() as usize;
                    for &face in &group.bsp_faces()[start..start + usize::from(node.face_count())] {
                        let face = usize::from(face);
                        if self.movement_faces[face]
                            || group.polygons()[face].flags() & exclusions != 0
                        {
                            continue;
                        }
                        self.movement_faces[face] = true;
                        selected += 1;
                        if selected > 8_192 {
                            if exclusions == 0x82 {
                                break;
                            }
                            return Err(MovementCollectionError::WorldModelFaceLimit);
                        }
                        let vertices = std::array::from_fn(|i| {
                            Vec3::from_array(
                                group.vertices()[usize::from(group.indices()[face * 3 + i])],
                            )
                        });
                        let admitted = if cached {
                            local.admits_cached_leaf(vertices)
                        } else {
                            local.admits(vertices, 0.0)
                        };
                        if admitted {
                            output.push(world_model_triangle(vertices, self.transform)?);
                        }
                    }
                    continue;
                }
                let axis = usize::from(node.flags() & 3);
                if region.minimum[axis] > query.maximum[axis]
                    || query.minimum[axis] > region.maximum[axis]
                {
                    continue;
                }
                let plane = node.plane_distance();
                // LIFO pushes negative first so the positive child runs first.
                if query.minimum[axis] <= plane
                    && let Ok(child) = usize::try_from(node.negative_child())
                {
                    let mut next_query = query;
                    let mut next_region = region;
                    next_query.maximum[axis] = next_query.maximum[axis].min(plane);
                    next_region.maximum[axis] = plane;
                    self.movement_pending.push(MovementBspQuery {
                        node: child,
                        query: next_query,
                        region: next_region,
                        depth: depth + 1,
                    });
                }
                if plane <= query.maximum[axis]
                    && let Ok(child) = usize::try_from(node.positive_child())
                {
                    let mut next_query = query;
                    let mut next_region = region;
                    next_query.minimum[axis] = next_query.minimum[axis].max(plane);
                    next_region.minimum[axis] = plane;
                    self.movement_pending.push(MovementBspQuery {
                        node: child,
                        query: next_query,
                        region: next_region,
                        depth: depth + 1,
                    });
                }
            }
        }
        Ok(())
    }
}
