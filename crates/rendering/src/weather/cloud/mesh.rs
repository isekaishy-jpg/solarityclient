//! Original cloud dome geometry and horizon alpha falloff.

/// One pole and eleven 16-vertex rings with radial texture coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldCloudDome {
    positions: [[f32; 3]; 177],
    coordinates: [[f32; 2]; 177],
    colors: [u32; 177],
    indices: [u16; 374],
}

impl Default for WorldCloudDome {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldCloudDome {
    /// Constructs 7F20E0's fixed unit cloud mesh.
    #[must_use]
    pub fn new() -> Self {
        const LATITUDES: [f32; 12] = [
            0., 0.025, 0.05, 0.075, 0.1, 0.125, 0.15, 0.175, 0.205, 0.23, 0.245, 0.25,
        ];
        const ALPHAS: [u32; 12] = [255, 255, 255, 255, 255, 255, 255, 255, 255, 128, 0, 0];
        let mut mesh = Self {
            positions: [[0.; 3]; 177],
            coordinates: [[0.; 2]; 177],
            colors: [0; 177],
            indices: [0; 374],
        };
        let offset = -(0.785_398_185_253_143_3_f64.cos() as f32);
        let mut vertex = 0;
        let mut previous = 0;
        for (ring, latitude) in LATITUDES.into_iter().enumerate() {
            let start = vertex;
            let angle = f64::from(latitude) * f64::from(std::f32::consts::PI);
            let horizontal = f64::from(angle.sin() as f32);
            let z = (angle.cos() + f64::from(offset)) as f32;
            let uv_radius = f64::from((ring as f64 * f64::from(1.0_f32 / 11.0) * 0.5) as f32);
            let mut longitude = 0.0_f32;
            for _ in 0..if ring == 0 { 1 } else { 16 } {
                let (sin, cos) = f64::from(longitude).sin_cos();
                mesh.positions[vertex] = [(sin * horizontal) as f32, (cos * horizontal) as f32, z];
                mesh.coordinates[vertex] = [
                    (sin * uv_radius + 0.5) as f32,
                    (cos * uv_radius + 0.5) as f32,
                ];
                mesh.colors[vertex] = 0x00ff_ffff | ALPHAS[ring] << 24;
                longitude += std::f32::consts::PI / 8.0;
                vertex += 1;
            }
            if ring > 0 {
                for segment in 0..17 {
                    let index = (ring - 1) * 34 + segment * 2;
                    mesh.indices[index] =
                        (previous + if ring == 1 { 0 } else { segment % 16 }) as u16;
                    mesh.indices[index + 1] = (start + segment % 16) as u16;
                }
            }
            previous = start;
        }
        mesh
    }

    /// Native positions relative to the camera-centered sky origin.
    #[must_use]
    pub const fn positions(&self) -> &[[f32; 3]; 177] {
        &self.positions
    }
    /// Radial procedural texture coordinates.
    #[must_use]
    pub const fn coordinates(&self) -> &[[f32; 2]; 177] {
        &self.coordinates
    }
    /// White vertex colors with the native horizon opacity falloff.
    #[must_use]
    pub const fn colors(&self) -> &[u32; 177] {
        &self.colors
    }
    /// One native triangle strip joined by repeated seam indices.
    #[must_use]
    pub const fn indices(&self) -> &[u16; 374] {
        &self.indices
    }
}
