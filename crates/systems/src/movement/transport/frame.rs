//! Passenger coordinates used by native Collide.cpp and Movement_C frame changes.

use glam::{Mat4, Vec3};
use thiserror::Error;

use super::MovementTransportChange;
use crate::collision::{MovementCollisionTriangle, MovementSweepError};

/// A finite affine parent matrix and its native rigid transpose conversion.
///
/// Stock 4C2FC0 transposes the stored basis; it does not compute a general matrix
/// inverse or repair a rounded rotation. The separate facing comes from the
/// parent's virtual angle and is not reconstructed from the matrix.
#[derive(Clone, Copy, Debug)]
pub struct MovementTransportFrame {
    world: Mat4,
    local: Mat4,
    facing: f32,
}

/// Invalid parent matrix or facing at the movement coordinate boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("movement transport frame is invalid")]
pub struct MovementTransportFrameError;

impl MovementTransportFrame {
    /// Builds Unit_C's unscaled local yaw frame (722B50/757BE0), optionally
    /// composing it with the current passenger parent through native 4C2370.
    ///
    /// # Errors
    /// Rejects non-finite poses or an unrepresentable composed matrix.
    pub fn unit(
        position: Vec3,
        facing: f32,
        parent: Option<Self>,
    ) -> Result<Self, MovementTransportFrameError> {
        let sine = f64::from(facing).sin() as f32;
        let cosine = f64::from(facing).cos() as f32;
        let rotation = Mat4::from_cols_array(&[
            cosine, sine, 0., 0., -sine, cosine, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ]);
        let local = unit_matrix_product(rotation, Mat4::from_translation(position));
        let (matrix, facing) = parent.map_or((local, facing), |parent| {
            (
                unit_matrix_product(local, parent.world),
                parent.world_orientation(facing),
            )
        });
        Self::new(matrix, facing)
    }

    /// Retains the exact parent basis and constructs native 4C2FC0's reverse map.
    ///
    /// # Errors
    /// Rejects non-finite, non-affine, or singular matrices and non-finite facing.
    pub fn new(world: Mat4, facing: f32) -> Result<Self, MovementTransportFrameError> {
        let determinant = world.determinant();
        if !world.is_finite()
            || !facing.is_finite()
            || !determinant.is_finite()
            || determinant == 0.0
            || world.x_axis.w != 0.0
            || world.y_axis.w != 0.0
            || world.z_axis.w != 0.0
            || world.w_axis.w != 1.0
        {
            return Err(MovementTransportFrameError);
        }
        let m = world.to_cols_array().map(f64::from);
        let [x, y, z] = [-m[12], -m[13], -m[14]];
        let local = Mat4::from_cols_array(&[
            m[0] as f32,
            m[4] as f32,
            m[8] as f32,
            0.,
            m[1] as f32,
            m[5] as f32,
            m[9] as f32,
            0.,
            m[2] as f32,
            m[6] as f32,
            m[10] as f32,
            0.,
            (z * m[2] + y * m[1] + x * m[0]) as f32,
            (y * m[5] + z * m[6] + x * m[4]) as f32,
            (x * m[8] + y * m[9] + z * m[10]) as f32,
            1.,
        ]);
        if !local.is_finite() {
            return Err(MovementTransportFrameError);
        }
        Ok(Self {
            world,
            local,
            facing,
        })
    }

    /// Returns the unmodified parent local-to-world matrix.
    #[must_use]
    pub const fn world_matrix(self) -> Mat4 {
        self.world
    }

    /// Returns the parent's independent world facing, without matrix decomposition.
    #[must_use]
    pub const fn facing(self) -> f32 {
        self.facing
    }

    /// Returns the native reverse matrix, including its separately rounded translation.
    #[must_use]
    pub const fn local_matrix(self) -> Mat4 {
        self.local
    }

    /// Converts retained analytic state when boarding this parent (98BA20).
    #[must_use]
    pub const fn entry_change(self) -> MovementTransportChange {
        MovementTransportChange::new(self.local, -self.facing)
    }

    /// Converts retained analytic state when leaving this parent (98B9A0).
    #[must_use]
    pub const fn exit_change(self) -> MovementTransportChange {
        MovementTransportChange::new(self.world, self.facing)
    }

    /// Converts a passenger-space foot point for world collection (4C2300).
    #[must_use]
    pub fn world_position(self, position: Vec3) -> Vec3 {
        point(self.world, position)
    }

    /// Converts a world point for passenger movement or attachment (4C2300).
    #[must_use]
    pub fn local_position(self, position: Vec3) -> Vec3 {
        point(self.local, position)
    }

    /// Converts travel before collection without renormalizing its length image.
    #[must_use]
    pub fn world_direction(self, direction: Vec3) -> Vec3 {
        let m = self.world.to_cols_array().map(f64::from);
        let [x, y, z] = direction.to_array().map(f64::from);
        Vec3::from_array(std::array::from_fn(|row| {
            (x * m[row] + y * m[4 + row] + z * m[8 + row]) as f32
        }))
    }

    /// Converts a world's selected face back to the passenger's solver space.
    /// Native 75FF90/75F0A0 preserve vertex order and do not normalize the normal.
    ///
    /// # Errors
    /// Rejects a transformed vertex or normal that cannot be represented finitely.
    pub fn local_triangle(
        self,
        triangle: &MovementCollisionTriangle,
    ) -> Result<MovementCollisionTriangle, MovementSweepError> {
        let m = self.local.to_cols_array().map(f64::from);
        let [x, y, z] = triangle.normal().to_array().map(f64::from);
        let normal = Vec3::from_array(std::array::from_fn(|row| {
            (y * m[4 + row] + z * m[8 + row] + x * m[row]) as f32
        }));
        MovementCollisionTriangle::with_normal(
            triangle
                .vertices()
                .map(|position| self.local_position(position)),
            normal,
        )
    }

    /// Adds the parent's independently supplied yaw and applies native 4C5090.
    #[must_use]
    pub fn world_orientation(self, orientation: f32) -> f32 {
        wrap_orientation((f64::from(orientation) + f64::from(self.facing)) as f32)
    }

    /// Removes parent yaw when admitting a world-space unit onto its new parent.
    #[must_use]
    pub fn local_orientation(self, orientation: f32) -> f32 {
        wrap_orientation((f64::from(orientation) - f64::from(self.facing)) as f32)
    }
}

/// 4C1F00's sixteen x87 dot products have distinct addition orders. Matrix
/// storage is row-major in stock and column-major in glam, reversing the product.
pub(crate) fn unit_matrix_product(local: Mat4, parent: Mat4) -> Mat4 {
    let a = local.to_cols_array().map(f64::from);
    let b = parent.to_cols_array().map(f64::from);
    const ORDER: [[usize; 4]; 16] = [
        [2, 1, 3, 0],
        [2, 1, 0, 3],
        [1, 3, 0, 2],
        [2, 0, 1, 3],
        [1, 2, 3, 0],
        [1, 2, 3, 0],
        [3, 2, 1, 0],
        [1, 3, 2, 0],
        [1, 2, 3, 0],
        [1, 3, 2, 0],
        [3, 2, 1, 0],
        [1, 3, 2, 0],
        [1, 2, 3, 0],
        [1, 3, 2, 0],
        [3, 2, 1, 0],
        [1, 3, 2, 0],
    ];
    Mat4::from_cols_array(&std::array::from_fn(|index| {
        let [i, j, k, l] = ORDER[index];
        let term = |lane| a[(index / 4) * 4 + lane] * b[lane * 4 + index % 4];
        (term(i) + term(j) + term(k) + term(l)) as f32
    }))
}

/// 4C2300 multiplies in x/y/z order, retaining x87 intermediates until each store.
pub(super) fn point(matrix: Mat4, position: Vec3) -> Vec3 {
    let m = matrix.to_cols_array().map(f64::from);
    let [x, y, z] = position.to_array().map(f64::from);
    Vec3::from_array(std::array::from_fn(|row| {
        (x * m[row] + y * m[4 + row] + z * m[8 + row] + m[12 + row]) as f32
    }))
}

/// 4C5090 uses a double containing the exact f32 2pi value, then adds that f32.
pub(super) fn wrap_orientation(orientation: f32) -> f32 {
    let period = f64::from(std::f32::consts::TAU);
    let remainder = f64::from(orientation) % period;
    (if remainder < 0.0 {
        remainder + period
    } else {
        remainder
    }) as f32
}
