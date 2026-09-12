//! Native six-vertex celestial strip and its horizon fade.

use super::WorldCelestialBody;
use glam::{Mat4, Vec3, Vec4};

/// One clipped native sun/moon strip, retaining the original packed colors.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldCelestialMesh {
    positions: [[f32; 3]; 6],
    uv: [[f32; 2]; 6],
    colors: [u32; 6],
    indices: [u16; 6],
    vertex_count: usize,
    index_count: usize,
}

impl WorldCelestialMesh {
    /// Runs 7EDBE0/7EDEE0. The native fade replaces alpha below 0.4 units,
    /// including weather alpha; it must not be multiplied into that alpha.
    #[must_use]
    pub fn new(body: WorldCelestialBody, eye: Vec3, color: u32) -> Self {
        let mut mesh = Self::unclipped(body.size(), color);
        mesh.clip(body, eye);
        mesh
    }

    /// 7EDBE0 also supplies the uncut disc used by glare occlusion queries;
    /// 9AC400's visible glare uses the same first four local positions.
    pub(crate) fn unclipped(size: f32, color: u32) -> Self {
        Self {
            positions: [
                [0., -0.5, 0.5],
                [0., 0.5, 0.5],
                [0., -0.5, -0.5],
                [0., 0.5, -0.5],
                [0., -0.5, 99.],
                [0., 0.5, 99.],
            ]
            .map(|p| p.map(|v| v * size)),
            uv: [[0., 0.], [1., 0.], [0., 1.], [1., 1.], [0., 99.], [1., 99.]],
            colors: [color; 6],
            indices: [0, 1, 2, 3, 0, 0],
            vertex_count: 4,
            index_count: 4,
        }
    }

    /// Applies only the ordinary celestial disc's horizon and alpha clipping.
    fn clip(&mut self, body: WorldCelestialBody, eye: Vec3) {
        let mesh = self;
        // x87 retains the unrounded height until the first alpha write, then
        // reloads its stored float for the subsequent vertices.
        let mut height = f64::from(body.position().z) - f64::from(eye.z);
        let top = f64::from(mesh.positions[0][2]) + height;
        let bottom = f64::from(mesh.positions[2][2]) + height;
        if top < 0. && bottom < 0. {
            mesh.vertex_count = 0;
            return;
        }
        if top <= 0. || bottom <= 0. {
            let ratio = top / (top - bottom);
            let z = ((f64::from(mesh.positions[2][2]) - f64::from(mesh.positions[0][2])) * ratio
                + f64::from(mesh.positions[0][2])) as f32;
            for i in [2, 3] {
                mesh.positions[i][2] = z;
                mesh.uv[i][1] = ratio as f32;
            }
        }
        let fade = f64::from(0.4_f32);
        let epsilon = f64::from(0.001_f32);
        let top = f64::from(mesh.positions[0][2]) + height - fade;
        let bottom = f64::from(mesh.positions[2][2]) + height - fade;
        if top > epsilon && bottom < epsilon {
            mesh.vertex_count = 6;
            mesh.index_count = 6;
            mesh.indices = [0, 1, 4, 5, 2, 3];
            let ratio = top / (top - bottom);
            let z = ((f64::from(mesh.positions[2][2]) - f64::from(mesh.positions[0][2])) * ratio
                + f64::from(mesh.positions[0][2])) as f32;
            let uv = ((f64::from(mesh.uv[2][1]) - f64::from(mesh.uv[0][1])) * ratio
                + f64::from(mesh.uv[0][1])) as f32;
            for i in [4, 5] {
                mesh.positions[i][2] = z;
                mesh.uv[i][1] = uv;
            }
        }
        for i in 0..mesh.vertex_count {
            let offset = height + f64::from(mesh.positions[i][2]) - fade;
            if offset < epsilon {
                let alpha = (((fade + offset) * 2.5).clamp(0., 1.) * 255.) as f32;
                mesh.colors[i] =
                    (mesh.colors[i] & 0x00ff_ffff) | (alpha.round_ties_even() as u32) << 24;
                height = f64::from(height as f32);
            }
        }
    }

    /// Native local XYZ positions, including the reserved fade vertices.
    #[must_use]
    pub const fn positions(&self) -> &[[f32; 3]; 6] {
        &self.positions
    }
    /// Texture coordinates clipped with the geometry at the horizon.
    #[must_use]
    pub const fn uv(&self) -> &[[f32; 2]; 6] {
        &self.uv
    }
    /// Native ARGB colors in vertex order.
    #[must_use]
    pub const fn colors(&self) -> &[u32; 6] {
        &self.colors
    }
    /// The admitted triangle strip; empty when the body is below the horizon.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices[..if self.vertex_count == 0 {
            0
        } else {
            self.index_count
        }]
    }
}

/// 9ABB60's orientation, including its axis-aligned fallback.
pub(super) fn basis(axis: Vec3) -> Mat4 {
    let [x, y, z] = axis.to_array().map(f64::from);
    let scale = 1. / (x * x + y * y + z * z).sqrt();
    let first = [(x * scale) as f32, (y * scale) as f32, (z * scale) as f32];
    let mut second = [-first[1], first[0], 0.];
    if (f64::from(second[0]) * (x * scale)).abs() <= f64::from(0.000_01_f32) {
        second = [0., 1., 0.];
    } else {
        let [a, b, _] = second.map(f64::from);
        let scale = 1. / (a * a + b * b).sqrt();
        second = [(a * scale) as f32, (b * scale) as f32, 0.];
    }
    let a = first.map(f64::from);
    let b = second.map(f64::from);
    let third = [
        (b[2] * a[1] - a[2] * b[1]) as f32,
        (a[2] * b[0] - b[2] * a[0]) as f32,
        (a[0] * b[1] - b[0] * a[1]) as f32,
    ];
    Mat4::from_cols(
        Vec3::from_array(first).extend(0.),
        Vec3::from_array(second).extend(0.),
        Vec3::from_array(third).extend(0.),
        Vec4::W,
    )
}

#[cfg(test)]
#[path = "../../../tests/stock_seed/celestial_mesh_native.rs"]
mod tests;
