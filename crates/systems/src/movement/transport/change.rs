//! Native 98B850 conversions of retained analytic state at a parent change.

use glam::{Mat4, Vec2, Vec3};

use super::frame::{point, wrap_orientation};

/// One admitted entry/exit conversion, retaining the parent's exact basis and yaw.
/// Obtain this from a validated [`super::MovementTransportFrame`].
#[derive(Clone, Copy, Debug)]
pub struct MovementTransportChange {
    matrix: Mat4,
    orientation_delta: f32,
}

impl MovementTransportChange {
    /// The frame owner has already validated the matrix and independent facing.
    pub(super) const fn new(matrix: Mat4, orientation_delta: f32) -> Self {
        Self {
            matrix,
            orientation_delta,
        }
    }

    /// Converts a retained position without changing its elapsed analytic time.
    #[must_use]
    pub fn position(self, position: Vec3) -> Vec3 {
        point(self.matrix, position)
    }

    /// Adds parent yaw at the native f32 store boundary before wrapping.
    #[must_use]
    pub fn orientation(self, orientation: f32) -> f32 {
        wrap_orientation((f64::from(orientation) + f64::from(self.orientation_delta)) as f32)
    }

    /// Rotates the full retained travel basis without normalizing or changing speed.
    #[must_use]
    pub fn direction(self, direction: Vec3) -> Vec3 {
        let m = self.matrix.to_cols_array().map(f64::from);
        let [x, y, z] = direction.to_array().map(f64::from);
        // 98B850 uses a different addition order for each stored component.
        Vec3::new(
            (z * m[8] + y * m[4] + x * m[0]) as f32,
            (x * m[1] + z * m[9] + y * m[5]) as f32,
            (y * m[6] + x * m[2] + z * m[10]) as f32,
        )
    }

    /// Converts the launch-height lane using the old current foot's horizontal position.
    #[must_use]
    pub fn launch_height(self, position: Vec3, height: f32) -> f32 {
        let m = self.matrix.to_cols_array().map(f64::from);
        (f64::from(position.y) * m[6]
            + f64::from(height) * m[10]
            + f64::from(position.x) * m[2]
            + m[14]) as f32
    }

    /// Converts the separate step-height lane in 98B850's native addition order.
    #[must_use]
    pub fn step_height(self, position: Vec3, height: f32) -> f32 {
        let m = self.matrix.to_cols_array().map(f64::from);
        (f64::from(position.y) * m[6]
            + f64::from(position.x) * m[2]
            + f64::from(height) * m[10]
            + m[14]) as f32
    }

    /// Produces the launch XY lane from the already converted full travel basis.
    /// Stock retains tiny vectors and normalizes only above the squared 2^-22 gate.
    #[must_use]
    pub fn horizontal_direction(direction: Vec3) -> Vec2 {
        let x = f64::from(direction.x);
        let y = f64::from(direction.y);
        let squared = x * x + y * y;
        if squared > 1.0 / 4_194_304.0 {
            let inverse = 1.0 / squared.sqrt();
            Vec2::new((x * inverse) as f32, (y * inverse) as f32)
        } else {
            direction.truncate()
        }
    }
}
