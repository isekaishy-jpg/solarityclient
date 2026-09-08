//! x87 float stores used by the camera's small matrix pipeline.

use glam::Vec3;

pub(super) const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

pub(super) fn normalize(vector: Vec3) -> Vec3 {
    (vector.as_dvec3() * (1.0 / vector.as_dvec3().length_squared().sqrt())).as_vec3()
}

pub(super) fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> [f32; 16] {
    let forward = normalize(target - eye);
    let side = normalize(forward.as_dvec3().cross(up.as_dvec3()).as_vec3());
    let up = normalize(side.as_dvec3().cross(forward.as_dvec3()).as_vec3());
    [
        side.x,
        up.x,
        forward.x,
        0.,
        side.y,
        up.y,
        forward.y,
        0.,
        side.z,
        up.z,
        forward.z,
        0.,
        -side.as_dvec3().dot(eye.as_dvec3()) as f32,
        -up.as_dvec3().dot(eye.as_dvec3()) as f32,
        -forward.as_dvec3().dot(eye.as_dvec3()) as f32,
        1.,
    ]
}

pub(super) fn projection(aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let diagonal = (f64::from(aspect) * f64::from(aspect) + 1.0) as f32;
    let scale = 1.0 / (f64::from(diagonal).sqrt() as f32);
    let fov = scale * std::f32::consts::FRAC_PI_2;
    let tangent = (f64::from(fov) * 0.5).tan() as f32;
    let n = f64::from(near);
    let f = f64::from(far);
    [
        (n / (n * f64::from(tangent) * f64::from(aspect))) as f32,
        0.,
        0.,
        0.,
        0.,
        (n / (n * f64::from(tangent))) as f32,
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

pub(super) fn inverse(matrix: [f32; 16]) -> Option<[f32; 16]> {
    let m = matrix.map(f64::from);
    let minor = |row: usize, column: usize| {
        let mut values = [0.0; 9];
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
    if !determinant.is_finite() || determinant.abs() < f32::EPSILON * 2. {
        return None;
    }
    let scale = 1.0 / determinant;
    Some(std::array::from_fn(|index| {
        let row = index % 4;
        let column = index / 4;
        let sign = if (row + column) & 1 == 0 { 1.0 } else { -1.0 };
        (sign * minor(row, column)) as f32 * scale
    }))
}

pub(super) fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|index| {
        (0..4)
            .map(|k| f64::from(a[index / 4 * 4 + k]) * f64::from(b[k * 4 + index % 4]))
            .sum::<f64>() as f32
    })
}

pub(super) fn vector4(matrix: [f32; 16], v: [f32; 4]) -> Vec3 {
    Vec3::from_array(std::array::from_fn(|index| {
        (0..4)
            .map(|k| f64::from(v[k]) * f64::from(matrix[k * 4 + index]))
            .sum::<f64>() as f32
    }))
}

pub(super) fn point(matrix: [f32; 16], v: Vec3) -> Vec3 {
    vector4(matrix, [v.x, v.y, v.z, 1.0])
}
