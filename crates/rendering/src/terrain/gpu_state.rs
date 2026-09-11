//! Explicit serialization for the terrain scene descriptor ABI.

use super::TerrainTextureAnimationState;
use glam::{Mat4, Vec3, Vec4};

/// Per-frame camera and outdoor directional-light state shared by all MCNKs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainSceneUniform {
    projection: Mat4,
    view: Mat4,
    ambient_color: Vec3,
    diffuse_color: Vec3,
    sun_direction: Vec3,
    view_depth: Vec4,
    fog_parameters: Vec4,
    fog_color: Vec4,
    specular_color_and_power: Vec4,
    texture_offsets: [Vec4; 64],
}

impl TerrainSceneUniform {
    /// Byte size of the exact std140 scene block consumed by both stages.
    pub const BYTE_SIZE: usize = 1264;

    /// Captures one camera/light snapshot without introducing lighting defaults.
    ///
    /// `sun_direction` points from the terrain surface toward the outdoor light.
    /// Terrain.bls applies view and projection separately: combining them loses
    /// depth precision when translating the original world-space MCNK vertices.
    #[must_use]
    pub const fn new(
        projection: Mat4,
        view: Mat4,
        ambient_color: Vec3,
        diffuse_color: Vec3,
        sun_direction: Vec3,
    ) -> Self {
        Self {
            projection,
            view,
            ambient_color,
            diffuse_color,
            sun_direction,
            view_depth: Vec4::ZERO,
            fog_parameters: Vec4::ZERO,
            fog_color: Vec4::ZERO,
            specular_color_and_power: Vec4::ZERO,
            texture_offsets: [Vec4::ZERO; 64],
        }
    }

    /// Captures all native MCLY direction/speed combinations once per frame.
    #[must_use]
    pub fn with_texture_animation(mut self, animation: &TerrainTextureAnimationState) -> Self {
        for (index, offset) in self.texture_offsets.iter_mut().enumerate() {
            *offset = animation
                .texture_offset(index as u32 | 0x40)
                .extend(0.0)
                .extend(0.0);
        }
        self
    }

    /// Enables Terrain.bls's vertex specular term with native power 20.
    /// The diffuse BLP alpha masks this independent light contribution.
    #[must_use]
    pub fn with_specular(mut self, color: Vec3, enabled: bool) -> Self {
        // 7CFBE0 supplies the directional light's specular RGB and A3FFF0.
        self.specular_color_and_power = if enabled {
            color.extend(20.0)
        } else {
            Vec4::ZERO
        };
        self
    }

    /// Enables the original Terrain.bls vertex fog using view-space depth.
    /// `parameters` contains start, end, an unused component, and exponent.
    #[must_use]
    pub fn with_fog(mut self, view: Mat4, parameters: Vec4, color: Vec3) -> Self {
        self.view_depth = view.row(2);
        self.fog_parameters = parameters;
        self.fog_color = color.extend(1.0);
        self
    }

    /// Serializes the shader block without relying on host layout or glam ABI.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        write_mat4(&mut bytes, &mut offset, self.projection);
        write_vec4(&mut bytes, &mut offset, self.ambient_color.extend(0.0));
        write_vec4(&mut bytes, &mut offset, self.diffuse_color.extend(0.0));
        write_vec4(&mut bytes, &mut offset, self.sun_direction.extend(0.0));
        write_vec4(&mut bytes, &mut offset, self.view_depth);
        write_vec4(&mut bytes, &mut offset, self.fog_parameters);
        write_vec4(&mut bytes, &mut offset, self.fog_color);
        write_mat4(&mut bytes, &mut offset, self.view);
        write_vec4(&mut bytes, &mut offset, self.specular_color_and_power);
        for texture_offset in self.texture_offsets {
            write_vec4(&mut bytes, &mut offset, texture_offset);
        }
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
