//! MO transport Euler matrices and canonical passenger rotation.

use glam::{Mat4, Vec3};
use solarity_ecs::GameObjectAnimatedPose;

use super::GameObjectPlacementError;

/// Constructs the two pose representations written by native `0x007134A0`.
///
/// Rotations apply Z yaw, Y pitch, then X roll, spilling the matrix after each
/// multiplication. Native `0x00982910`/`0x004F43B0` independently derive the
/// packed passenger rotation; rebuilding the matrix from it loses precision.
///
/// # Errors
/// Returns [`GameObjectPlacementError::InvalidTransform`] for non-finite inputs.
pub fn game_object_transport_pose(
    position: Vec3,
    yaw: f32,
    pitch: f32,
    roll: f32,
) -> Result<GameObjectAnimatedPose, GameObjectPlacementError> {
    if !position.is_finite() || ![yaw, pitch, roll].into_iter().all(f32::is_finite) {
        return Err(GameObjectPlacementError::InvalidTransform);
    }
    // FSINCOS in 4C31B0/4C3220/4C3290 stores both outputs as floats.
    let (sy, cy) = f64::from(yaw).sin_cos();
    let (sp, cp) = f64::from(pitch).sin_cos();
    let (sr, cr) = f64::from(roll).sin_cos();
    let [sy, cy, sp, cp, sr, cr] = [sy, cy, sp, cp, sr, cr].map(|v| f64::from(v as f32));
    let yaw_pitch = [
        (cy * cp) as f32,
        (sy * cp) as f32,
        -sp as f32,
        -sy as f32,
        cy as f32,
        0.0,
        (cy * sp) as f32,
        (sy * sp) as f32,
        cp as f32,
    ]
    .map(f64::from);
    let mut matrix = [0.0; 16];
    for row in 0..3 {
        matrix[row] = yaw_pitch[row] as f32;
        matrix[4 + row] = (cr * yaw_pitch[3 + row] + sr * yaw_pitch[6 + row]) as f32;
        matrix[8 + row] = (-sr * yaw_pitch[3 + row] + cr * yaw_pitch[6 + row]) as f32;
    }
    matrix[12..15].copy_from_slice(&position.to_array());
    matrix[15] = 1.0;
    let rotation = matrix_rotation(matrix);
    Ok(GameObjectAnimatedPose::new(
        Mat4::from_cols_array(&matrix),
        pack_rotation(rotation),
    ))
}

/// 4F43B0 packs either an MO matrix rotation or a type-11 sampled quaternion.
pub(super) fn pack_rotation(rotation: [f32; 4]) -> u64 {
    let sign = if rotation[3] >= 0.0 { 1 } else { -1 };
    // 407930 truncates to i64 and returns its low word. The float multiplication
    // is stored first; hemisphere IMUL subsequently wraps at 32 bits.
    let x = truncate_rotation_component(rotation[0] * 2_097_152.0).wrapping_mul(sign) as u64;
    let y = truncate_rotation_component(rotation[1] * 1_048_576.0).wrapping_mul(sign) as u64;
    let z = truncate_rotation_component(rotation[2] * 1_048_576.0).wrapping_mul(sign) as u64;
    (x << 42) | ((y & 0x1f_ffff) << 21) | (z & 0x1f_ffff)
}

fn truncate_rotation_component(value: f32) -> i32 {
    let value = f64::from(value).trunc();
    if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) {
        value as i64 as i32
    } else {
        // FISTP's i64 integer-indefinite value has a zero low word.
        0
    }
}

/// 9827B0 takes the transposed matrix and an independently spilled trace.
fn matrix_rotation(matrix: [f32; 16]) -> [f32; 4] {
    let m = matrix.map(f64::from);
    let trace = f64::from(((m[10] + m[5]) + m[0]) as f32);
    if trace > 0.0 {
        let root = (trace + 1.0).sqrt();
        let factor = 0.5 / root;
        return [
            ((m[6] - m[9]) * factor) as f32,
            ((m[8] - m[2]) * factor) as f32,
            ((m[1] - m[4]) * factor) as f32,
            (root * 0.5) as f32,
        ];
    }
    let mut i = usize::from(m[0] < m[5]);
    if m[i * 5] < m[10] {
        i = 2;
    }
    let j = (i + 1) % 3;
    let k = (j + 1) % 3;
    let root = (((m[i * 5] - m[j * 5]) - m[k * 5]) + 1.0).sqrt();
    let factor = 0.5 / root;
    let mut rotation = [0.0; 4];
    rotation[i] = (root * 0.5) as f32;
    rotation[3] = ((m[k + j * 4] - m[j + k * 4]) * factor) as f32;
    rotation[j] = ((m[j + i * 4] + m[i + j * 4]) * factor) as f32;
    rotation[k] = ((m[k + i * 4] + m[i + k * 4]) * factor) as f32;
    rotation
}
