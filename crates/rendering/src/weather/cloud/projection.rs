//! Native ray/sphere projection of celestial light into cloud texture space.

use super::WorldCloudLighting;
use glam::Vec3;

/// 7EF920 projects an unscaled world direction into the native 128-square dome.
pub(super) fn opacity_coordinates(eye: Vec3, source: Vec3) -> [f32; 2] {
    let x = f64::from(source.x) - f64::from(eye.x);
    let y = f64::from(source.y) - f64::from(eye.y);
    let z = f64::from(source.z) - f64::from(eye.z) + 0.785_398_185_253_143_3_f64.cos();
    let angle = (z / (y * y + z * z + x * x).sqrt())
        .acos()
        .min(f64::from(std::f32::consts::FRAC_PI_4));
    let radial = angle * f64::from(1.273_239_5_f32) * 0.5;
    let x = f64::from(x as f32);
    let y = f64::from(y as f32);
    let length = (x * x + y * y).sqrt();
    let (x, y) = if length > 0.000_01 {
        (x / length, y / length)
    } else {
        (0., 0.)
    };
    [
        (128. * (x * radial + 0.5)) as f32,
        (128. * (y * radial + 0.5)) as f32,
    ]
}

impl WorldCloudLighting {
    /// Samples 7EFAE0 from ambient/diffuse/emissive cloud bands, day, camera and celestials.
    /// `weather_blend` is the native retained precipitation/cloud attenuation scalar.
    #[must_use]
    pub fn sample(
        colors: [Vec3; 3],
        day: f32,
        eye: Vec3,
        sun: Vec3,
        moon: Vec3,
        weather_blend: f32,
    ) -> Self {
        let source = if (0.201_388_9..=0.923_611_1).contains(&day) {
            sun
        } else {
            moon
        };
        let ray = (source - eye).to_array();
        let cos = 0.785_398_185_253_143_3_f64.cos();
        let [x, y, z] = ray.map(f64::from);
        let a = f64::from((z * z + y * y + x * x) as f32);
        let b = f64::from((cos * z * 2.0) as f32);
        let c = f64::from((cos * cos - 1.0) as f32);
        let discriminant = (b * b - 4.0 * a * c).sqrt();
        let q = -0.5
            * if b <= 0.0 {
                b - discriminant
            } else {
                b + discriminant
            };
        let inverse = 1.0 / (q * a);
        let parameter = (inverse * q * q).max(inverse * a * c) as f32;
        let point = std::array::from_fn::<_, 3, _>(|i| {
            (f64::from(ray[i]) * f64::from(parameter) + f64::from(eye[i])) as f32
        });
        let x = f64::from(point[0]) - f64::from(eye.x);
        let y = f64::from(point[1]) - f64::from(eye.y);
        let z = f64::from(point[2]) - f64::from(eye.z) + cos;
        let angle = (z / (z * z + y * y + x * x).sqrt())
            .acos()
            .min(f64::from(std::f32::consts::FRAC_PI_4));
        let radial = angle * f64::from(1.273_239_5_f32) * 0.5;
        let x = f64::from(x as f32);
        let y = f64::from(y as f32);
        let length = (x * x + y * y).sqrt();
        let (x, y) = if length > 0.000_01 {
            (x / length, y / length)
        } else {
            (0.0, 0.0)
        };
        let position = [
            (128.0 * (x * radial + 0.5)) as f32,
            (128.0 * (y * radial + 0.5)) as f32,
            (f64::from(weather_blend) * 192.0 + 64.0) as f32,
        ];
        let [ambient, diffuse, emissive] = colors.map(|color| {
            color
                .to_array()
                .map(|v| (v * 255.0).round_ties_even() * (1.0_f32 / 255.0))
        });
        // Every value in the native eight-key AF4BE0 table is one.
        let strength = (1.0 - f64::from(weather_blend) * 0.75) as f32;
        Self::new(ambient, diffuse, emissive, position, strength)
    }
}
