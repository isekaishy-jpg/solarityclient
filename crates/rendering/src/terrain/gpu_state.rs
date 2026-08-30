//! Explicit serialization for the terrain scene descriptor ABI.

use glam::{Mat4, Vec3, Vec4};

/// Per-frame camera and outdoor directional-light state shared by all MCNKs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainSceneUniform {
    view_projection: Mat4,
    ambient_color: Vec3,
    diffuse_color: Vec3,
    sun_direction: Vec3,
}

impl TerrainSceneUniform {
    /// Byte size of the exact std140 scene block consumed by both stages.
    pub const BYTE_SIZE: usize = 112;

    /// Captures one camera/light snapshot without introducing lighting defaults.
    ///
    /// `sun_direction` points from the terrain surface toward the outdoor light.
    #[must_use]
    pub const fn new(
        view_projection: Mat4,
        ambient_color: Vec3,
        diffuse_color: Vec3,
        sun_direction: Vec3,
    ) -> Self {
        Self {
            view_projection,
            ambient_color,
            diffuse_color,
            sun_direction,
        }
    }

    /// Serializes the shader block without relying on host layout or glam ABI.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        write_mat4(&mut bytes, &mut offset, self.view_projection);
        write_vec4(&mut bytes, &mut offset, self.ambient_color.extend(0.0));
        write_vec4(&mut bytes, &mut offset, self.diffuse_color.extend(0.0));
        write_vec4(&mut bytes, &mut offset, self.sun_direction.extend(0.0));
        debug_assert_eq!(offset, Self::BYTE_SIZE);
        bytes
    }
}

fn write_mat4(bytes: &mut [u8; TerrainSceneUniform::BYTE_SIZE], offset: &mut usize, value: Mat4) {
    for column in value.to_cols_array_2d() {
        write_vec4(bytes, offset, Vec4::from_array(column));
    }
}

fn write_vec4(bytes: &mut [u8; TerrainSceneUniform::BYTE_SIZE], offset: &mut usize, value: Vec4) {
    for component in value.to_array() {
        let end = *offset + size_of::<f32>();
        bytes[*offset..end].copy_from_slice(&component.to_le_bytes());
        *offset = end;
    }
}
