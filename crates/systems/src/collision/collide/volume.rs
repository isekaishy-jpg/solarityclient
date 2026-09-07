//! The native nine-plane body: four sides, a top, and four sloped foot faces.

use glam::{DVec3, Vec3};

use super::sweep::MovementSweepError;

const FOOT_HEIGHT_PER_RADIUS: f32 = f32::from_bits(0x3fec_b91b);
const FOOT_NORMAL_HORIZONTAL: f32 = f32::from_bits(0x3f61_3036);
const FOOT_NORMAL_VERTICAL: f32 = f32::from_bits(0x3ef3_86a4);

/// An outward body plane with the interior at `normal.dot(point) + offset <= 0`.
///
/// Contact planes belong to the moving body; they are not world-triangle normals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementCollisionPlane {
    normal: Vec3,
    offset: f32,
}

impl MovementCollisionPlane {
    pub(super) const ZERO: Self = Self {
        normal: Vec3::ZERO,
        offset: 0.0,
    };

    /// Constructs an already-normalized plane at its owning geometry boundary.
    pub(super) fn through(normal: Vec3, point: Vec3) -> Self {
        Self::through_extended(normal.as_dvec3(), point)
    }

    /// x87 retains the normalized components through offset calculation before
    /// writing the four plane floats (0x0075BF78--0x0075BFBB).
    pub(super) fn through_extended(normal: DVec3, point: Vec3) -> Self {
        Self {
            normal: normal.as_vec3(),
            offset: -normal.dot(point.as_dvec3()) as f32,
        }
    }

    /// Returns the outward normal in the query's coordinate space.
    #[must_use]
    pub const fn normal(self) -> Vec3 {
        self.normal
    }

    /// Returns the signed constant term of the plane equation.
    #[must_use]
    pub const fn offset(self) -> f32 {
        self.offset
    }

    pub(super) fn distance(self, point: Vec3) -> f64 {
        self.normal.as_dvec3().dot(point.as_dvec3()) + f64::from(self.offset)
    }
}

/// Fixed player-volume geometry from `0x0075C8F0` and `0x0075CA80`.
///
/// Dimensions are supplied by the movement owner in its current coordinate
/// space. Transport transforms and the swimming height adjustment precede this
/// boundary; this type does not infer either from packet flags.
#[derive(Clone, Copy, Debug)]
pub struct MovementCollisionVolume {
    pub(super) planes: [MovementCollisionPlane; 9],
    pub(super) vertices: [Vec3; 9],
    pub(super) radius: f32,
    pub(super) height: f32,
}

impl MovementCollisionVolume {
    /// Supplies the admitted foot position to movement's contact-time owner.
    pub(crate) const fn foot_origin(&self) -> Vec3 {
        self.vertices[0]
    }

    /// Builds the body at the foot origin with the supplied radius and height.
    ///
    /// # Errors
    /// Returns [`MovementSweepError::InvalidVolume`] for non-finite coordinates,
    /// non-positive radius, or a top below the sloped foot's upper edge. These
    /// are geometric admission requirements, not substitutes for unit dimensions.
    pub fn new(origin: Vec3, radius: f32, height: f32) -> Result<Self, MovementSweepError> {
        let foot_height = radius * FOOT_HEIGHT_PER_RADIUS;
        if !origin.is_finite()
            || !radius.is_finite()
            || radius <= 0.0
            || !height.is_finite()
            || height < foot_height
        {
            return Err(MovementSweepError::InvalidVolume);
        }
        let vertices = [
            origin,
            origin + Vec3::new(-radius, -radius, foot_height),
            origin + Vec3::new(-radius, radius, foot_height),
            origin + Vec3::new(radius, radius, foot_height),
            origin + Vec3::new(radius, -radius, foot_height),
            origin + Vec3::new(-radius, -radius, height),
            origin + Vec3::new(-radius, radius, height),
            origin + Vec3::new(radius, radius, height),
            origin + Vec3::new(radius, -radius, height),
        ];
        let x = FOOT_NORMAL_HORIZONTAL;
        let z = -FOOT_NORMAL_VERTICAL;
        let planes = [
            MovementCollisionPlane::through(-Vec3::X, origin - Vec3::X * radius),
            MovementCollisionPlane::through(Vec3::X, origin + Vec3::X * radius),
            MovementCollisionPlane::through(Vec3::Y, origin + Vec3::Y * radius),
            MovementCollisionPlane::through(-Vec3::Y, origin - Vec3::Y * radius),
            MovementCollisionPlane::through(Vec3::Z, origin + Vec3::Z * height),
            MovementCollisionPlane::through(Vec3::new(-x, 0.0, z), origin),
            MovementCollisionPlane::through(Vec3::new(x, 0.0, z), origin),
            MovementCollisionPlane::through(Vec3::new(0.0, x, z), origin),
            MovementCollisionPlane::through(Vec3::new(0.0, -x, z), origin),
        ];
        if vertices.iter().any(|point| !point.is_finite())
            || planes.iter().any(|plane| !plane.offset.is_finite())
        {
            return Err(MovementSweepError::InvalidVolume);
        }
        Ok(Self {
            planes,
            vertices,
            radius,
            height,
        })
    }
}
