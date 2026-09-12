//! Original 7A9090 near-portal test and 7A85E0 transformed polygon clipping.

use super::WorldModelPortalProjectionFrame;
use crate::collision::{
    world_model_portal::polygon_contains, world_model_visibility::WorldModelVisibilityError,
};
use glam::Vec3;
use solarity_asset::DecodedWorldModel;

const EPSILON: f32 = 0.0001;

/// Reusable polygon clip buffers and projected windows for an admitted WMO.
#[derive(Default)]
pub struct WorldModelPortalProjector {
    buffers: [Vec<Vec3>; 2],
    windows: Vec<Option<[f32; 4]>>,
    projected: Vec<bool>,
}

impl WorldModelPortalProjector {
    /// Validate the complete admitted input before lazy scene traversal. A
    /// rejected or malformed portal must not be confused with an unvisited one.
    pub(in crate::collision::world_model_visibility) fn begin_scene(
        &mut self,
        model: &DecodedWorldModel,
        frame: WorldModelPortalProjectionFrame,
    ) -> Result<(), WorldModelVisibilityError> {
        self.windows.clear();
        self.projected.clear();
        if !frame.is_finite() {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        for portal in model.portals() {
            let start = usize::from(portal.vertex_start());
            let end = start + usize::from(portal.vertex_count());
            if !portal.normal().into_iter().all(f32::is_finite)
                || !portal.distance().is_finite()
                || !model.portal_vertices()[start..end]
                    .iter()
                    .flatten()
                    .copied()
                    .all(f32::is_finite)
            {
                return Err(WorldModelVisibilityError::NonFiniteCoordinates);
            }
        }
        self.windows.resize(model.portals().len(), None);
        self.projected.resize(model.portals().len(), false);
        Ok(())
    }

    /// Uses the same polygon arithmetic as eager projection, once per portal
    /// reached by this query. Begin a new scene before changing model or frame.
    pub(in crate::collision::world_model_visibility) fn scene_portal(
        &mut self,
        model: &DecodedWorldModel,
        frame: WorldModelPortalProjectionFrame,
        index: usize,
    ) -> Result<Option<[f32; 4]>, WorldModelVisibilityError> {
        if !self.projected[index] {
            let portal = model.portals()[index];
            let start = usize::from(portal.vertex_start());
            let end = start + usize::from(portal.vertex_count());
            let [x, y, z] = portal.normal();
            let window = self.project_polygon(
                &model.portal_vertices()[start..end],
                [x, y, z, portal.distance()],
                frame,
            )?;
            if window.is_some_and(|window| !window.into_iter().all(f32::is_finite)) {
                return Err(WorldModelVisibilityError::NonFiniteCoordinates);
            }
            self.windows[index] = window;
            self.projected[index] = true;
        }
        Ok(self.windows[index])
    }

    /// Projects every authored portal, retaining capacity between frames.
    ///
    /// The optional native occlusion provider is disabled at this boundary.
    /// Returned windows use min-Y, min-X, max-Y, max-X order and can be passed
    /// directly to `WorldModelVisibilityQuery::query`.
    ///
    /// # Errors
    /// Rejects nonfinite camera, transform, plane or polygon coordinates.
    pub fn project(
        &mut self,
        model: &DecodedWorldModel,
        frame: WorldModelPortalProjectionFrame,
    ) -> Result<&[Option<[f32; 4]>], WorldModelVisibilityError> {
        self.windows.clear();
        if !frame.is_finite() {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        for portal in model.portals() {
            let start = usize::from(portal.vertex_start());
            let end = start + usize::from(portal.vertex_count());
            let [x, y, z] = portal.normal();
            let window = self.project_polygon(
                &model.portal_vertices()[start..end],
                [x, y, z, portal.distance()],
                frame,
            )?;
            self.windows.push(window);
        }
        Ok(&self.windows)
    }

    /// Projects one root-local polygon through native near, clip and divide rules.
    ///
    /// Near-plane inclusion considers the whole authored polygon. Ordinary
    /// projection transforms at most its first twelve vertices, as 7A85E0 does.
    ///
    /// # Errors
    /// Rejects nonfinite input coordinates.
    pub fn project_polygon(
        &mut self,
        vertices: &[[f32; 3]],
        plane: [f32; 4],
        frame: WorldModelPortalProjectionFrame,
    ) -> Result<Option<[f32; 4]>, WorldModelVisibilityError> {
        if !frame.is_finite()
            || !plane.into_iter().all(f32::is_finite)
            || !vertices.iter().flatten().copied().all(f32::is_finite)
        {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        let distance = plane_distance(frame.local_camera, plane);
        if distance > -f64::from(0.01_f32)
            && distance < f64::from(0.01_f32)
            && polygon_contains(frame.local_camera, vertices, Vec3::from_slice(&plane[..3]))
        {
            return Ok(Some([-1., -1., 1., 1.]));
        }
        self.project_clipped_polygon(vertices, Vec3::ZERO, frame)
    }

    /// Shared 7A85E0 projection after its caller chooses the local offset and
    /// handles any near-portal shortcut. Inputs have already been validated.
    pub(super) fn project_clipped_polygon(
        &mut self,
        vertices: &[[f32; 3]],
        offset: Vec3,
        frame: WorldModelPortalProjectionFrame,
    ) -> Result<Option<[f32; 4]>, WorldModelVisibilityError> {
        self.buffers[0].clear();
        let root = frame.root_transform.to_cols_array_2d();
        self.buffers[0].extend(vertices.iter().take(12).map(|&point| {
            // The local offset addition spills before the root transform.
            let point = (Vec3::from_array(point) + offset).to_array();
            // 4C21B0 retains its sums until each output component is stored.
            Vec3::from_array(std::array::from_fn(|r| {
                (((f64::from(root[2][r]) * f64::from(point[2])
                    + f64::from(root[1][r]) * f64::from(point[1]))
                    + f64::from(root[0][r]) * f64::from(point[0]))
                    + f64::from(root[3][r])) as f32
            }))
        }));
        for (index, clip) in frame.clip_planes.into_iter().enumerate() {
            let [first, second] = &mut self.buffers;
            let (input, output) = if index & 1 == 0 {
                (first, second)
            } else {
                (second, first)
            };
            output.clear();
            for (i, &current) in input.iter().enumerate() {
                let next = input[(i + 1) % input.len()];
                // 7A72A0 retains the extended distance for classification and
                // independently stores it as float for edge intersections.
                let distance = clip_distance(current, clip);
                let next_distance = clip_distance(next, clip);
                let side = classify(distance);
                let next_side = classify(next_distance);
                if side >= 0 {
                    output.push(current);
                }
                if side != 0 && next_side != 0 && side != next_side {
                    let distance = f64::from(distance as f32);
                    let next_distance = f64::from(next_distance as f32);
                    // Combine the interpolation numerator before dividing so
                    // f64 does not round the fraction before a cancellation.
                    output.push(
                        ((next.as_dvec3() * distance - current.as_dvec3() * next_distance)
                            / (distance - next_distance))
                            .as_vec3(),
                    );
                }
            }
            if output.is_empty() {
                return Ok(None);
            }
        }
        let vertices = &self.buffers[1];
        if vertices.len() < 3 {
            return Ok(None);
        }
        let matrix = frame.relative_projection.to_cols_array_2d();
        let mut bounds = [f32::MAX, f32::MAX, -f32::MAX, -f32::MAX];
        for &vertex in vertices {
            // The camera subtraction spills before the four-vector transform.
            let p = (vertex - frame.world_camera).to_array().map(f64::from);
            let component = |r: usize| {
                let m = [matrix[0][r], matrix[1][r], matrix[2][r], matrix[3][r]].map(f64::from);
                (if r == 0 {
                    ((m[3] + m[1] * p[1]) + m[2] * p[2]) + m[0] * p[0]
                } else {
                    ((m[3] + m[1] * p[1]) + m[0] * p[0]) + m[2] * p[2]
                }) as f32
            };
            let inverse_w = 1. / f64::from(component(3).max(EPSILON));
            let x = (inverse_w * f64::from(component(0))) as f32;
            let y = (inverse_w * f64::from(component(1))) as f32;
            bounds[0] = bounds[0].min(y);
            bounds[1] = bounds[1].min(x);
            bounds[2] = bounds[2].max(y);
            bounds[3] = bounds[3].max(x);
        }
        Ok(Some(bounds))
    }
}

fn plane_distance(point: Vec3, plane: [f32; 4]) -> f64 {
    let p = point.as_dvec3();
    let [x, y, z, d] = plane.map(f64::from);
    ((y * p.y + z * p.z) + x * p.x) + d
}

fn clip_distance(point: Vec3, plane: [f32; 4]) -> f64 {
    let p = point.as_dvec3();
    let [x, y, z, d] = plane.map(f64::from);
    ((x * p.x + z * p.z) + y * p.y) + d
}

fn classify(distance: f64) -> i8 {
    if distance > f64::from(EPSILON) {
        1
    } else if distance < -f64::from(EPSILON) {
        -1
    } else {
        0
    }
}
