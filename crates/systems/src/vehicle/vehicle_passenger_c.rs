//! Model placement owned by VehiclePassenger_C, separate from movement frames.

use crate::movement::unit_matrix_product;
use glam::{Mat4, Vec3};

/// Native A2D3F0 maps the seat enum to a model attachment ID.
#[must_use]
pub fn vehicle_seat_attachment(seat_attachment: i32) -> Option<u32> {
    const IDS: [u32; 22] = [
        20, 34, 19, 21, 22, 17, 23, 24, 25, 15, 16, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 0,
    ];
    IDS.get(seat_attachment as u32 as usize).copied()
}

/// Inputs to native 7490F0 after attachment lookup and body-yaw resolution.
#[derive(Clone, Copy, Debug)]
pub struct VehicleSeatPose {
    /// Passenger body yaw in the vehicle's facing frame.
    pub passenger_yaw: f32,
    /// Authored yaw, pitch and roll.
    pub rotation: Vec3,
    /// Authored seat displacement.
    pub offset: Vec3,
    /// Static passenger-model attachment position, when present.
    pub passenger_anchor: Option<Vec3>,
    /// The passenger's own model scale.
    pub passenger_scale: f32,
    /// Vehicle unit scale; only multiplies offsets without a model attachment.
    pub vehicle_scale: f32,
    /// Animated vehicle attachment matrix, including its world placement.
    pub attachment: Option<Mat4>,
    /// Vehicle position and facing fallback; excludes terrain/model tilt and scale.
    pub vehicle_position: Vec3,
    /// Vehicle facing, or the transition controller's retained facing.
    pub vehicle_yaw: f32,
}

/// Resolves the seated passenger's world model matrix (7490F0).
/// The body keeps its own scale even when an attachment bone changes scale.
#[must_use]
pub fn vehicle_seat_transform(pose: VehicleSeatPose) -> Mat4 {
    let local = seat_local(pose);
    let parent = pose.attachment.unwrap_or_else(|| {
        unit_matrix_product(
            rotation_z(pose.vehicle_yaw),
            Mat4::from_translation(pose.vehicle_position),
        )
    });
    unit_matrix_product(local, parent)
}

/// 7493B0's entry destination: attachment position in the vehicle's movement
/// frame, with the static passenger anchor applied only when it exists.
#[must_use]
pub fn vehicle_entry_target(
    pose: VehicleSeatPose,
    model_frame: Mat4,
    movement_frame: Mat4,
    parent_transition: bool,
) -> Vec3 {
    let offset = if pose.passenger_anchor.is_some() {
        if pose.attachment.is_none() {
            return vehicle_seat_transform(pose).w_axis.truncate();
        }
        seat_local(pose).w_axis.truncate()
    } else {
        pose.offset
    };
    let Some(attachment) = pose.attachment else {
        return point(
            if parent_transition {
                model_frame
            } else {
                movement_frame
            },
            offset,
        );
    };
    let target = point(attachment, offset);
    if parent_transition {
        return target;
    }
    if model_frame.z_axis.z <= 0.999 {
        return target - model_frame.w_axis.truncate() + movement_frame.w_axis.truncate();
    }
    let local = point(scaled_inverse(model_frame, pose.vehicle_scale), target);
    let mut movement = movement_frame;
    movement.x_axis *= pose.vehicle_scale;
    movement.y_axis *= pose.vehicle_scale;
    movement.z_axis *= pose.vehicle_scale;
    point(movement, local)
}

fn scaled_inverse(matrix: Mat4, scale: f32) -> Mat4 {
    let mut inverse = matrix.transpose();
    inverse.x_axis.w = 0.;
    inverse.y_axis.w = 0.;
    inverse.z_axis.w = 0.;
    inverse.w_axis = glam::Vec4::W;
    if (f64::from(scale) - 1.).abs() < f64::from(0.000_000_953_674_3_f32) {
        let m = matrix.to_cols_array().map(f64::from);
        let [x, y, z] = [-m[12], -m[13], -m[14]];
        inverse.w_axis = Vec3::new(
            (z * m[2] + y * m[1] + x * m[0]) as f32,
            (y * m[5] + z * m[6] + x * m[4]) as f32,
            (x * m[8] + y * m[9] + z * m[10]) as f32,
        )
        .extend(1.);
        return inverse;
    }
    let factor = 1. / (f64::from(scale) * f64::from(scale));
    for column in 0..3 {
        for row in 0..3 {
            inverse.col_mut(column)[row] = (f64::from(inverse.col(column)[row]) * factor) as f32;
        }
    }
    inverse.w_axis = point(inverse, -matrix.w_axis.truncate()).extend(1.);
    inverse
}

fn point(matrix: Mat4, position: Vec3) -> Vec3 {
    let m = matrix.to_cols_array().map(f64::from);
    let [x, y, z] = position.to_array().map(f64::from);
    Vec3::from_array(std::array::from_fn(|row| {
        (x * m[row] + y * m[4 + row] + z * m[8 + row] + m[12 + row]) as f32
    }))
}

fn seat_local(pose: VehicleSeatPose) -> Mat4 {
    let mut local = unit_matrix_product(
        rotation_z(pose.passenger_yaw - pose.rotation.x),
        Mat4::IDENTITY,
    );
    if pose.rotation.z != 0.0 {
        let (s, c) = sin_cos(pose.rotation.z);
        let rotation =
            Mat4::from_cols_array(&[1., 0., 0., 0., 0., c, s, 0., 0., -s, c, 0., 0., 0., 0., 1.]);
        local = unit_matrix_product(rotation, local);
    }
    if pose.rotation.y != 0.0 {
        let (s, c) = sin_cos(pose.rotation.y);
        let rotation =
            Mat4::from_cols_array(&[c, 0., -s, 0., 0., 1., 0., 0., s, 0., c, 0., 0., 0., 0., 1.]);
        local = unit_matrix_product(rotation, local);
    }
    let (scale, offset_scale) =
        pose.attachment
            .map_or((pose.passenger_scale, pose.vehicle_scale), |attachment| {
                let [x, y, z] = attachment.x_axis.truncate().to_array().map(f64::from);
                let length = (x * x + y * y + z * z).sqrt() as f32;
                let scale = if length <= 0.000_1 {
                    pose.passenger_scale
                } else {
                    (f64::from(pose.passenger_scale) / f64::from(length)) as f32
                };
                (scale, 1.)
            });
    local.x_axis *= scale;
    local.y_axis *= scale;
    local.z_axis *= scale;
    let anchor = pose.passenger_anchor.map_or(Vec3::ZERO, |position| {
        let m = local.to_cols_array().map(f64::from);
        let [x, y, z] = (-position).to_array().map(f64::from);
        Vec3::from_array(std::array::from_fn(|row| {
            (x * m[row] + y * m[4 + row] + z * m[8 + row] + m[12 + row]) as f32
        }))
    });
    let offset = Vec3::from_array(std::array::from_fn(|lane| {
        (f64::from(pose.offset[lane]) * f64::from(offset_scale) + f64::from(anchor[lane])) as f32
    }));
    local.w_axis = offset.extend(1.);
    local
}

fn sin_cos(angle: f32) -> (f32, f32) {
    (f64::from(angle).sin() as f32, f64::from(angle).cos() as f32)
}

fn rotation_z(angle: f32) -> Mat4 {
    let (s, c) = sin_cos(angle);
    Mat4::from_cols_array(&[c, s, 0., 0., -s, c, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.])
}
