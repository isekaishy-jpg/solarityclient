//! Horizontal fall response from `0x0075E9C0` and its edge/plane helpers.

use glam::{DVec3, Vec2, Vec3};

use super::{
    CONTACT_TOLERANCE, DEGENERATE_TOLERANCE, DIRECTION_TOLERANCE, MovementCollisionPlane,
    MovementCollisionTriangle, MovementCollisionVolume,
};

const STEEP_LIMIT: f32 = f32::from_bits(0x3f24_8dbb);
const RESPONSE_BIAS: f32 = f32::from_bits(0x3a83_126f);

impl MovementCollisionVolume {
    /// Computes the correction added to remaining XY travel, retaining Z.
    pub(super) fn fall_correction(
        &self,
        triangle: &MovementCollisionTriangle,
        planes: &[MovementCollisionPlane],
        direction: Vec3,
        distance: f32,
        maximum: f32,
    ) -> Vec2 {
        let normal = if triangle.normal.z.abs() > STEEP_LIMIT {
            let point = (self.vertices[0].as_dvec3() + direction.as_dvec3() * f64::from(distance))
                .as_vec3();
            triangle.nearest_edge_normal(point, direction)
        } else {
            self.fall_plane_normal(triangle, planes)
        }
        .as_dvec3();
        let mut horizontal = normal.truncate();
        if horizontal.length_squared() > f64::from(DIRECTION_TOLERANCE) {
            horizontal = horizontal.normalize();
        }
        let mut correction =
            (f64::from(maximum) - f64::from(distance)) * -normal.dot(direction.as_dvec3());
        let denominator = normal.truncate().dot(horizontal);
        if denominator.abs() >= f64::from(DIRECTION_TOLERANCE) {
            correction /= denominator;
        }
        (horizontal * (correction + f64::from(RESPONSE_BIAS))).as_vec2()
    }

    /// The two-body-plane branch can resolve a triangle edge at their three
    /// plane intersection. All other finite branches retain the world normal.
    fn fall_plane_normal(
        &self,
        triangle: &MovementCollisionTriangle,
        planes: &[MovementCollisionPlane],
    ) -> Vec3 {
        // 0x0075D890 takes ABS of both plane distances before multiplying them
        // and testing < 0 (0x0075DAAB..0x0075DAC8). Its one-contact branch
        // therefore never selects the negated body normal for finite geometry.
        if planes.len() != 2 {
            return triangle.normal;
        }
        let first = planes[0];
        let second = planes[1];
        let line = first.normal().as_dvec3().cross(second.normal().as_dvec3());
        let line = line.normalize().as_vec3();
        if line.z.abs() < DEGENERATE_TOLERANCE || !line.is_finite() {
            return triangle.normal;
        }
        let dot = line.as_dvec3().dot(triangle.normal.as_dvec3()) as f32;
        let plane = MovementCollisionPlane::through(triangle.normal, triangle.vertices[0]);
        let touches_foot = self.vertices[..5]
            .iter()
            .any(|&point| plane.distance(point).abs() < f64::from(CONTACT_TOLERANCE));
        if dot.abs() < DEGENERATE_TOLERANCE
            || ((line.z - 1.0).abs() >= DEGENERATE_TOLERANCE && touches_foot)
        {
            return triangle.normal;
        }
        let point = three_plane_intersection([plane, first, second]);
        let edge = triangle.nearest_edge_to_line(point, line);
        let normal = line.as_dvec3().cross(edge.as_dvec3()).as_vec3();
        let length = normal.as_dvec3().length() as f32;
        if length.abs() < DIRECTION_TOLERANCE {
            return -first.normal();
        }
        let normal = (normal.as_dvec3() / f64::from(length)).as_vec3();
        if normal.as_dvec3().dot(first.normal().as_dvec3()) > 0.0 {
            -normal
        } else {
            normal
        }
    }
}

impl MovementCollisionTriangle {
    /// Distance is to each infinite edge line, not a clamped edge segment.
    fn nearest_edge_normal(&self, point: Vec3, direction: Vec3) -> Vec3 {
        let mut nearest = f64::INFINITY;
        let mut selected = DVec3::ZERO;
        for index in 0..3 {
            let origin = self.vertices[index].as_dvec3();
            let mut edge = self.vertices[(index + 1) % 3].as_dvec3() - origin;
            if edge.length_squared() > f64::from(DIRECTION_TOLERANCE) {
                edge = edge.normalize();
            }
            let projection = origin + edge * (point.as_dvec3() - origin).dot(edge);
            let distance = (point.as_dvec3() - projection).length_squared();
            if distance < nearest {
                nearest = distance;
                selected = edge;
            }
        }
        let normal = selected.cross(self.normal.as_dvec3()).as_vec3();
        if normal.as_dvec3().dot(direction.as_dvec3()) < 0.0 {
            -normal
        } else {
            normal
        }
    }

    /// Native `0x0075D4B0` uses an unnormalized cross-product metric. Only
    /// near-parallel +1 takes the point-to-line branch; -1 retains that metric.
    fn nearest_edge_to_line(&self, point: Vec3, line: Vec3) -> Vec3 {
        let mut nearest = f64::from(f32::MAX);
        let mut selected = Vec3::ZERO;
        let line = line.as_dvec3();
        let point = point.as_dvec3();
        for index in 0..3 {
            let origin = self.vertices[index].as_dvec3();
            let edge = origin - self.vertices[(index + 1) % 3].as_dvec3();
            let length = edge.length();
            if length.abs() < f64::from(DIRECTION_TOLERANCE) {
                continue;
            }
            let edge = edge / length;
            let distance = if (line.dot(edge) - 1.0).abs() >= f64::from(DEGENERATE_TOLERANCE) {
                let normal = edge.cross(line);
                (normal.dot(origin) - normal.dot(point)).powi(2)
            } else {
                (origin - (point + line * (origin - point).dot(line))).length_squared()
            };
            if distance < nearest {
                nearest = distance;
                selected = edge.as_vec3();
            }
        }
        selected
    }
}

/// The native matrix inverse stores cofactors, reciprocal determinant, and
/// each inverse element as floats before multiplying the plane offsets.
fn three_plane_intersection(planes: [MovementCollisionPlane; 3]) -> Vec3 {
    let [a, b, c] = planes.map(|plane| plane.normal().as_dvec3());
    let determinant = (((c.x * a.y * b.z + a.z * b.x * c.y) + c.z * a.x * b.y) - c.x * a.z * b.y)
        - c.z * a.y * b.x
        - a.x * c.y * b.z;
    let inverse = 1.0 / determinant as f32;
    let columns = [b.cross(c), c.cross(a), a.cross(b)]
        .map(|cofactor| (cofactor.as_vec3() * inverse).as_dvec3());
    (columns[0] * f64::from(-planes[0].offset())
        + columns[1] * f64::from(-planes[1].offset())
        + columns[2] * f64::from(-planes[2].offset()))
    .as_vec3()
}
