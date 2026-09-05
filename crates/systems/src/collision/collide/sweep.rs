//! Sweeps body faces against already-selected world triangles.

use glam::Vec3;
use thiserror::Error;

use super::face::extruded_edge_plane;
use super::polygon::ContactPolygon;
use super::{
    CONTACT_TOLERANCE, DEGENERATE_TOLERANCE, DIRECTION_TOLERANCE, MINIMUM_SWEEP_LENGTH,
    MovementCollisionPlane, MovementCollisionVolume,
};

// Vertex order and native iteration order from 0x0075CA80 / 0x0075F9D0.
const SIDE_FACES: [[usize; 4]; 5] = [
    [1, 2, 6, 5],
    [3, 4, 8, 7],
    [2, 3, 7, 6],
    [4, 1, 5, 8],
    [5, 6, 7, 8],
];
const FOOT_FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 3, 4], [0, 2, 3], [0, 4, 1]];
const TRIANGLE_APPROACH_LIMIT: f32 = f32::from_bits(0xb727_c5ac);
const PENETRATION_LIMIT: f32 = f32::from_bits(0xbce3_8e39);

/// Immutable query inputs shared by all nine body faces.
struct FaceSweep<'a> {
    direction: Vec3,
    extrusion: Vec3,
    maximum: f32,
    triangles: &'a [MovementCollisionTriangle],
}

/// One oriented triangle admitted by world collision selection.
#[derive(Clone, Copy, Debug)]
pub struct MovementCollisionTriangle {
    pub(super) vertices: [Vec3; 3],
    pub(super) normal: Vec3,
}

impl MovementCollisionTriangle {
    /// Preserves winding and derives its front-facing unit normal.
    ///
    /// # Errors
    /// Returns [`MovementSweepError::InvalidTriangle`] for non-finite or
    /// degenerate geometry. Material filtering belongs to the world provider.
    pub fn new(vertices: [Vec3; 3]) -> Result<Self, MovementSweepError> {
        let cross = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]);
        let length = cross.length();
        if vertices.iter().any(|point| !point.is_finite()) || !length.is_finite() || length == 0.0 {
            return Err(MovementSweepError::InvalidTriangle);
        }
        Ok(Self {
            vertices,
            normal: cross / length,
        })
    }
}

/// Invalid geometry or displacement at the pure movement contact boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MovementSweepError {
    /// Body dimensions or translated vertices cannot form a finite volume.
    #[error("movement collision volume is invalid")]
    InvalidVolume,
    /// A world triangle is non-finite or degenerate.
    #[error("movement collision triangle is invalid")]
    InvalidTriangle,
    /// Displacement or its length is non-finite.
    #[error("movement collision displacement is invalid")]
    InvalidDisplacement,
    /// A landing/support foot point is NaN or infinite.
    #[error("movement support point is not finite")]
    NonFiniteSupportPoint,
}

/// Travel allowed by the native narrow phase and its simultaneous body contacts.
#[derive(Clone, Copy, Debug)]
pub struct MovementSweep {
    distance: f32,
    planes: [MovementCollisionPlane; 9],
    count: usize,
    last_triangle: Option<usize>,
}

impl MovementSweep {
    /// Returns allowed travel in the query's coordinate units.
    #[must_use]
    pub const fn distance(self) -> f32 {
        self.distance
    }

    /// Returns body planes near the earliest contact, with the earliest first.
    #[must_use]
    pub fn planes(&self) -> &[MovementCollisionPlane] {
        &self.planes[..self.count]
    }

    /// Returns the last face query's selected triangle, as at `0x0075F9D0`.
    /// This native output is not necessarily the globally earliest triangle.
    #[must_use]
    pub const fn last_triangle(self) -> Option<usize> {
        self.last_triangle
    }

    /// Retains contacts within the stock distance tolerance and moves a newly
    /// earlier plane to the front without re-sorting the other contacts.
    fn admit(&mut self, distance: f32, plane: MovementCollisionPlane) {
        if distance < self.distance - CONTACT_TOLERANCE {
            self.planes[0] = plane;
            self.count = 1;
        } else if distance < self.distance + CONTACT_TOLERANCE {
            self.planes[self.count] = plane;
            self.count += 1;
        }
        if distance < self.distance {
            self.distance = distance;
            if self.count > 1 {
                self.planes.swap(0, self.count - 1);
            }
        }
    }
}

impl MovementCollisionVolume {
    /// Resolves one displacement through the native face-sweep narrow phase.
    ///
    /// The caller supplies the ordered, filtered triangle set in this volume's
    /// coordinate space. This implements `0x0075F9D0` after world collection;
    /// sliding, step-up, gravity, and resident-world lookup remain caller work.
    /// It allocates no memory and never converts missing geometry to a floor.
    ///
    /// # Errors
    /// Returns [`MovementSweepError::InvalidDisplacement`] for non-finite travel.
    pub fn sweep(
        &self,
        displacement: Vec3,
        triangles: &[MovementCollisionTriangle],
    ) -> Result<MovementSweep, MovementSweepError> {
        // Native callers retain products and their sum in x87 before storing
        // the length. Early f32 rounding can reorder simultaneous foot contacts.
        let distance = displacement.as_dvec3().length() as f32;
        if !displacement.is_finite() || !distance.is_finite() {
            return Err(MovementSweepError::InvalidDisplacement);
        }
        let mut result = MovementSweep {
            distance,
            planes: [MovementCollisionPlane::ZERO; 9],
            count: 0,
            last_triangle: None,
        };
        if distance < DEGENERATE_TOLERANCE {
            result.distance = 0.0;
            return Ok(result);
        }
        let direction = displacement / distance;
        let extrusion = direction * distance.max(MINIMUM_SWEEP_LENGTH);
        let query = FaceSweep {
            direction,
            extrusion,
            maximum: distance,
            triangles,
        };
        for (index, vertices) in FOOT_FACES.iter().enumerate() {
            self.sweep_face(index + 5, vertices, &query, &mut result);
        }
        for (index, vertices) in SIDE_FACES.iter().enumerate() {
            self.sweep_face(index, vertices, &query, &mut result);
        }
        if result.distance < CONTACT_TOLERANCE {
            result.distance = 0.0;
        }
        Ok(result)
    }

    /// Extrudes a leading face and clips each approaching triangle to its sides.
    /// The end-cap plane built by stock is unused by this particular query.
    fn sweep_face(
        &self,
        face: usize,
        indices: &[usize],
        query: &FaceSweep<'_>,
        result: &mut MovementSweep,
    ) {
        let plane = self.planes[face];
        if plane.normal().as_dvec3().dot(query.extrusion.as_dvec3()) <= 0.0 {
            return;
        }
        let mut sides = [MovementCollisionPlane::ZERO; 4];
        for (edge, &index) in indices.iter().enumerate() {
            let origin = self.vertices[index];
            let next = self.vertices[indices[(edge + 1) % indices.len()]];
            let previous = self.vertices[indices[(edge + indices.len() - 1) % indices.len()]];
            let Some(side) = extruded_edge_plane(origin, next, previous, query.extrusion) else {
                return;
            };
            sides[edge] = side;
        }

        let mut nearest = query.maximum;
        let mut selected = None;
        for (index, triangle) in query.triangles.iter().enumerate() {
            if triangle.normal.as_dvec3().dot(query.direction.as_dvec3())
                > f64::from(TRIANGLE_APPROACH_LIMIT)
            {
                continue;
            }
            let mut polygon = ContactPolygon::from_triangle(triangle.vertices);
            for side in &sides[..indices.len()] {
                polygon.clip(*side);
            }
            if let Some(distance) = self.face_contact(&mut polygon, face, query.direction)
                && distance <= nearest
            {
                nearest = distance;
                selected = Some(index);
            }
        }
        if let Some(index) = selected {
            result.last_triangle = Some(index);
            result.admit(nearest, plane);
        }
    }

    /// Finds first contact from clipped vertices, then tests initial penetration
    /// against the other eight body planes (`0x0075C0B0`).
    fn face_contact(
        &self,
        polygon: &mut ContactPolygon,
        face: usize,
        direction: Vec3,
    ) -> Option<f32> {
        if polygon.vertices().is_empty() {
            return None;
        }
        let plane = self.planes[face];
        let denominator = plane.normal().as_dvec3().dot(direction.as_dvec3());
        let distance_to_face = |point| {
            let distance = plane.distance(point);
            if denominator.abs() >= f64::from(DIRECTION_TOLERANCE) {
                distance / denominator
            } else {
                distance
            }
        };
        let mut nearest = f64::from(f32::MAX);
        let mut penetrating = true;
        for &point in polygon.vertices() {
            let distance = distance_to_face(point);
            nearest = nearest.min(distance.max(0.0));
            if distance > -f64::from(DEGENERATE_TOLERANCE) {
                penetrating = false;
            }
        }
        if penetrating {
            for (index, other) in self.planes.iter().enumerate() {
                if index != face {
                    polygon.clip(*other);
                }
                if polygon.vertices().is_empty() {
                    return None;
                }
            }
            if polygon
                .vertices()
                .iter()
                .all(|&point| distance_to_face(point) <= f64::from(PENETRATION_LIMIT))
            {
                return None;
            }
        }
        (nearest < f64::from(f32::MAX)).then_some(nearest as f32)
    }
}
