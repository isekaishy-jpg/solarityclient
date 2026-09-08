//! Original 7A9090 portal projection without the optional occlusion provider.

use glam::{Mat4, Vec3};
use solarity_asset::DecodedWorldModel;

use super::WorldModelVisibilityError;
use crate::collision::world_model_portal::polygon_contains;

const EPSILON: f32 = 0.0001;

/// Camera and placement inputs retained by native WMO portal projection.
#[derive(Clone, Copy, Debug)]
pub struct WorldModelPortalProjectionFrame {
    /// Root-local to world transform.
    pub root_transform: Mat4,
    /// Camera position in root-local coordinates, used by the near-portal test.
    pub local_camera: Vec3,
    /// World camera position subtracted before projection.
    pub world_camera: Vec3,
    /// View/projection matrix operating on world positions relative to the eye.
    pub relative_projection: Mat4,
    /// Inward world-space planes ordered top, bottom, right, left, far.
    /// The near plane is deliberately excluded from portal clipping.
    pub clip_planes: [[f32; 4]; 5],
}

impl WorldModelPortalProjectionFrame {
    /// Builds the five stock portal planes from world-space frustum corners.
    ///
    /// Both faces use bottom-left, top-left, top-right, bottom-right order in
    /// clip space, with the near face first. The corners must come from the
    /// stock positive-forward view convention, whose basis is mirrored relative
    /// to the usual left-handed view. Corner coordinates already include the eye.
    /// The supplied projection operates on positions relative to that eye.
    ///
    /// # Errors
    /// Rejects nonfinite inputs or a frustum with degenerate clipping faces.
    pub fn from_frustum_corners(
        root_transform: Mat4,
        local_camera: Vec3,
        world_camera: Vec3,
        relative_projection: Mat4,
        corners: [Vec3; 8],
    ) -> Result<Self, WorldModelVisibilityError> {
        if corners.iter().any(|corner| !corner.is_finite()) {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        let mut clip_planes = [[0.; 4]; 5];
        // 983E70 passes these corner triplets to 7912C0 in this order.
        for (output, [a, b, c]) in
            clip_planes
                .iter_mut()
                .zip([[1, 5, 6], [0, 7, 4], [0, 4, 5], [3, 6, 7], [5, 4, 6]])
        {
            let origin = corners[a].as_dvec3();
            // Differences remain extended, but the cross product spills to
            // floats before normalization. D uses the unspilled unit normal.
            let cross = (corners[b].as_dvec3() - origin)
                .cross(corners[c].as_dvec3() - origin)
                .as_vec3()
                .as_dvec3();
            let length_squared = (cross.x * cross.x + cross.y * cross.y) + cross.z * cross.z;
            if !length_squared.is_finite() || length_squared == 0. {
                return Err(WorldModelVisibilityError::DegenerateFrustum);
            }
            let inverse_length = 1. / length_squared.sqrt();
            let normal = cross * inverse_length;
            // Factor the common reciprocal out of D's dot product to avoid
            // losing the x87 cancellation precision when the eye is near zero.
            let distance =
                -((cross.y * origin.y + origin.x * cross.x) + origin.z * cross.z) * inverse_length;
            *output = [
                normal.x as f32,
                normal.y as f32,
                normal.z as f32,
                distance as f32,
            ];
        }
        let frame = Self {
            root_transform,
            local_camera,
            world_camera,
            relative_projection,
            clip_planes,
        };
        if !frame.is_finite() {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        Ok(frame)
    }

    fn is_finite(self) -> bool {
        self.root_transform.is_finite()
            && self.relative_projection.is_finite()
            && self.local_camera.is_finite()
            && self.world_camera.is_finite()
            && self.clip_planes.into_iter().flatten().all(f32::is_finite)
    }
}

/// Reusable polygon clip buffers and projected windows for an admitted WMO.
#[derive(Default)]
pub struct WorldModelPortalProjector {
    buffers: [Vec<Vec3>; 2],
    windows: Vec<Option<[f32; 4]>>,
}

impl WorldModelPortalProjector {
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
        self.buffers[0].clear();
        let root = frame.root_transform.to_cols_array_2d();
        self.buffers[0].extend(vertices.iter().take(12).map(|&point| {
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
