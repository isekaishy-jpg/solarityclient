//! Float stores from 6BFE60, 6BF370 and the scene's 4C matrix pipeline.

use glam::Vec3;

/// Native 4C3420 retains its reciprocal through all three component stores.
fn normalized(vector: Vec3) -> Vec3 {
    let vector = vector.as_dvec3();
    let inverse_length =
        1. / ((vector.x * vector.x + vector.y * vector.y) + vector.z * vector.z).sqrt();
    (vector * inverse_length).as_vec3()
}

/// 6BFE60 constructs the positive-forward, mirrored view relative to the eye.
pub(super) fn view(direction: Vec3, up: Vec3) -> [f32; 16] {
    let length_squared = |v: Vec3| {
        let v = v.as_dvec3();
        (v.z * v.z + v.x * v.x) + v.y * v.y
    };
    // 6BFE60 leaves its initialized identity for short direction/up vectors.
    if length_squared(direction) < f64::from(0.01_f32) || length_squared(up) < f64::from(0.01_f32) {
        return glam::Mat4::IDENTITY.to_cols_array();
    }
    let forward = normalized(direction);
    let side = normalized(forward.as_dvec3().cross(up.as_dvec3()).as_vec3());
    let up = normalized(side.as_dvec3().cross(forward.as_dvec3()).as_vec3());
    [
        side.x, up.x, forward.x, 0., side.y, up.y, forward.y, 0., side.z, up.z, forward.z, 0., 0.,
        0., 0., 1.,
    ]
}

/// 6BF370 spills the half-angle, tangent and near-plane extents to float.
pub(super) fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let half_angle = fov * 0.5;
    let tangent = f64::from(half_angle).tan() as f32;
    let vertical = near * tangent;
    let horizontal = vertical * aspect;
    let n = f64::from(near);
    let f = f64::from(far);
    [
        near / horizontal,
        0.,
        0.,
        0.,
        0.,
        near / vertical,
        0.,
        0.,
        0.,
        0.,
        ((n + f) / (f - n)) as f32,
        1.,
        0.,
        0.,
        ((-2. * f * n) / (f - n)) as f32,
        0.,
    ]
}

/// 4C1930 -> 4C2F90 stores determinant, cofactors and reciprocal separately.
pub(super) fn inverse(matrix: [f32; 16]) -> Option<[f32; 16]> {
    let m = matrix.map(f64::from);
    let minor = |row: usize, column: usize| {
        let mut values = [0.; 9];
        let mut index = 0;
        for r in 0..4 {
            for c in 0..4 {
                if r != row && c != column {
                    values[index] = m[r * 4 + c];
                    index += 1;
                }
            }
        }
        let [a, b, c, d, e, f, g, h, i] = values;
        (((a * e * i + c * d * h + b * f * g) - c * e * g) - b * d * i) - a * f * h
    };
    let determinant = ((minor(0, 2) * m[2] + (minor(0, 0) * m[0] - minor(0, 1) * m[1]))
        - minor(0, 3) * m[3]) as f32;
    if !determinant.is_finite() || determinant == 0. {
        return None;
    }
    let scale = 1. / determinant;
    Some(std::array::from_fn(|index| {
        let row = index % 4;
        let column = index / 4;
        let sign = if (row + column) & 1 == 0 { 1. } else { -1. };
        (sign * minor(row, column)) as f32 * scale
    }))
}

/// 4C1F00 combines row-major native matrices with component-specific sum order.
pub(super) fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
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
    std::array::from_fn(|index| {
        let product = |k| f64::from(a[index / 4 * 4 + k]) * f64::from(b[k * 4 + index % 4]);
        let [first, second, third, fourth] = ORDER[index];
        (((product(first) + product(second)) + product(third)) + product(fourth)) as f32
    })
}

/// 4C2270 followed by 982950 copies XYZ without dividing by homogeneous W.
pub(super) fn corner(matrix: [f32; 16], point: [f32; 4]) -> Vec3 {
    Vec3::from_array(std::array::from_fn(|index| {
        let product = |k| f64::from(point[k]) * f64::from(matrix[k * 4 + index]);
        (if index == 0 {
            ((product(3) + product(1)) + product(2)) + product(0)
        } else {
            ((product(3) + product(1)) + product(0)) + product(2)
        }) as f32
    }))
}

/// 4C21B0 adds Z/Y products first before the X product and translation.
pub(super) fn point(matrix: [f32; 16], point: Vec3) -> Vec3 {
    let p = point.as_dvec3();
    Vec3::from_array(std::array::from_fn(|axis| {
        (((p.z * f64::from(matrix[8 + axis]) + p.y * f64::from(matrix[4 + axis]))
            + p.x * f64::from(matrix[axis]))
            + f64::from(matrix[12 + axis])) as f32
    }))
}
