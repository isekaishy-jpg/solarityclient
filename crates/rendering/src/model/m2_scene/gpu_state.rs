//! Explicit byte serialization for the M2 shader descriptor and push ABI.

use glam::{Mat4, Vec3, Vec4};

use super::M2DrawCall;

/// One directional or positional local light in the stock four-light bound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2LocalLightState {
    position: Vec4,
    ambient: Vec4,
    diffuse: Vec4,
    attenuation: Vec4,
}

impl M2LocalLightState {
    /// Creates a directional light; `direction` points from the vertex to light.
    #[must_use]
    pub fn directional(direction: Vec3, ambient: Vec3, diffuse: Vec3) -> Self {
        Self {
            position: direction.extend(0.0),
            ambient: ambient.extend(0.0),
            diffuse: diffuse.extend(0.0),
            attenuation: Vec4::ZERO,
        }
    }

    /// Creates a positional light with constant, linear, and quadratic falloff.
    #[must_use]
    pub fn positional(position: Vec3, ambient: Vec3, diffuse: Vec3, attenuation: Vec3) -> Self {
        Self {
            position: position.extend(1.0),
            ambient: ambient.extend(0.0),
            diffuse: diffuse.extend(0.0),
            attenuation: attenuation.extend(0.0),
        }
    }

    /// Returns a disabled zero-contribution light for unused fixed slots.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            position: Vec4::ZERO,
            ambient: Vec4::ZERO,
            diffuse: Vec4::ZERO,
            attenuation: Vec4::ZERO,
        }
    }

    /// Appends the exact 64-byte std140 light struct.
    fn append_bytes(self, bytes: &mut Vec<u8>) {
        append_vec4(bytes, self.position);
        append_vec4(bytes, self.ambient);
        append_vec4(bytes, self.diffuse);
        append_vec4(bytes, self.attenuation);
    }
}

/// Per-frame scene block shared by every visible M2 draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2SceneUniform {
    view_projection: Mat4,
    camera_position: Vec3,
    ambient_light: Vec3,
    diffuse_light: Vec3,
    light_direction: Vec3,
    fog_parameters: Vec4,
    local_lights: [M2LocalLightState; 4],
}

impl M2SceneUniform {
    /// Byte size of the exact std140 scene descriptor block.
    pub const BYTE_SIZE: usize = 400;

    /// Creates one bounded scene-lighting and fog snapshot.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        view_projection: Mat4,
        camera_position: Vec3,
        ambient_light: Vec3,
        diffuse_light: Vec3,
        light_direction: Vec3,
        fog_parameters: Vec4,
        local_lights: [M2LocalLightState; 4],
    ) -> Self {
        Self {
            view_projection,
            camera_position,
            ambient_light,
            diffuse_light,
            light_direction,
            fog_parameters,
            local_lights,
        }
    }

    /// Serializes without depending on Rust or glam's in-memory representation.
    #[must_use]
    pub fn to_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::BYTE_SIZE);
        append_mat4(&mut bytes, self.view_projection);
        append_vec4(&mut bytes, self.camera_position.extend(1.0));
        append_vec4(&mut bytes, self.ambient_light.extend(0.0));
        append_vec4(&mut bytes, self.diffuse_light.extend(0.0));
        append_vec4(&mut bytes, self.light_direction.extend(0.0));
        append_vec4(&mut bytes, self.fog_parameters);
        for light in self.local_lights {
            light.append_bytes(&mut bytes);
        }
        bytes
    }
}

/// Per-material transforms, color, fog, and alpha threshold block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2MaterialUniform {
    model: Mat4,
    texture_transforms: [Mat4; 2],
    environment_view: Mat4,
    mesh_color: Vec4,
    fog_color: Vec4,
    fragment_parameters: Vec4,
}

impl M2MaterialUniform {
    /// Byte size of the exact std140 material descriptor block.
    pub const BYTE_SIZE: usize = 304;

    /// Creates one immutable material snapshot for a submitted draw.
    #[must_use]
    pub const fn new(
        model: Mat4,
        texture_transforms: [Mat4; 2],
        environment_view: Mat4,
        mesh_color: Vec4,
        fog_color: Vec4,
        fragment_parameters: Vec4,
    ) -> Self {
        Self {
            model,
            texture_transforms,
            environment_view,
            mesh_color,
            fog_color,
            fragment_parameters,
        }
    }

    /// Serializes without relying on host struct layout or alignment.
    #[must_use]
    pub fn to_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::BYTE_SIZE);
        append_mat4(&mut bytes, self.model);
        for transform in self.texture_transforms {
            append_mat4(&mut bytes, transform);
        }
        append_mat4(&mut bytes, self.environment_view);
        append_vec4(&mut bytes, self.mesh_color);
        append_vec4(&mut bytes, self.fog_color);
        append_vec4(&mut bytes, self.fragment_parameters);
        bytes
    }
}

/// Per-draw 16-byte push block selecting bones, texture stages, and behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2DrawPushConstants {
    bone_transform_offset: u32,
    bone_count: u32,
    texture_count: u32,
    flags: u32,
}

impl M2DrawPushConstants {
    /// Byte size shared with the Vulkan pipeline layout.
    pub const BYTE_SIZE: usize = 16;

    /// Creates the push block from a validated draw and instance palette base.
    #[must_use]
    pub fn new(bone_transform_offset: u32, draw: &M2DrawCall, flags: u32) -> Self {
        Self {
            bone_transform_offset,
            bone_count: u32::from(draw.bone_count()),
            texture_count: u32::from(draw.batch().texture_count),
            flags,
        }
    }

    /// Serializes four native-independent little-endian words.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let words = [
            self.bone_transform_offset,
            self.bone_count,
            self.texture_count,
            self.flags,
        ];
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        for (index, word) in words.into_iter().enumerate() {
            let start = index * size_of::<u32>();
            bytes[start..start + size_of::<u32>()].copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }
}

/// Appends one column-major matrix in GLSL's default layout.
fn append_mat4(bytes: &mut Vec<u8>, matrix: Mat4) {
    for value in matrix.to_cols_array() {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

/// Appends one tightly packed 16-byte std140 vector.
fn append_vec4(bytes: &mut Vec<u8>, vector: Vec4) {
    for value in vector.to_array() {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}
