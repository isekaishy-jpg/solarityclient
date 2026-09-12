//! Explicit byte serialization for the M2 shader descriptor and push ABI.

use glam::{Mat4, Vec3, Vec4};

use super::M2DrawCall;
use super::shadow::M2ShadowState;

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

    /// Writes the exact 64-byte std140 light struct into its scene block.
    fn write_bytes<const N: usize>(self, bytes: &mut [u8; N], offset: &mut usize) {
        write_vec4(bytes, offset, self.position);
        write_vec4(bytes, offset, self.ambient);
        write_vec4(bytes, offset, self.diffuse);
        write_vec4(bytes, offset, self.attenuation);
    }
}

/// Per-frame scene block shared by every visible M2 draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2SceneUniform {
    projection: Mat4,
    view: Mat4,
    camera_position: Vec3,
    ambient_light: Vec3,
    diffuse_light: Vec3,
    light_direction: Vec3,
    fog_parameters: Vec4,
    fog_enabled: bool,
    specular_enabled: bool,
    fog_color: Vec3,
    local_lights: [M2LocalLightState; 4],
    shadow: M2ShadowState,
    /// Positive eye depth, independent of perspective or parallel projection.
    view_depth_plane: Vec4,
}

impl M2SceneUniform {
    /// Byte size of the exact std140 scene descriptor block.
    pub const BYTE_SIZE: usize = 848;

    /// Creates one bounded scene-lighting and fog snapshot. Stock projects
    /// view-space positions; preserving that boundary avoids world-coordinate
    /// cancellation in clip Z. `view` also transforms world-space effect meshes.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        projection: Mat4,
        view: Mat4,
        camera_position: Vec3,
        ambient_light: Vec3,
        diffuse_light: Vec3,
        light_direction: Vec3,
        fog_parameters: Vec4,
        fog_color: Vec3,
        local_lights: [M2LocalLightState; 4],
    ) -> Self {
        Self {
            projection,
            view,
            camera_position,
            ambient_light,
            diffuse_light,
            light_direction,
            fog_parameters,
            fog_enabled: fog_parameters.y > fog_parameters.x,
            specular_enabled: true,
            fog_color,
            local_lights,
            shadow: M2ShadowState::disabled(),
            view_depth_plane: -view.row(2),
        }
    }

    /// Adds the constants required by a stock shadowed M2 permutation.
    #[must_use]
    pub const fn with_shadow(mut self, shadow: M2ShadowState) -> Self {
        self.shadow = shadow;
        self
    }

    /// Joins the primary map to the model/view coordinates emitted by Diffuse_T1.
    pub(crate) fn with_world_shadow(
        mut self,
        projection: crate::WorldShadowProjection,
        environment: Option<crate::WorldEnvironmentShadowFrame<'_>>,
    ) -> Self {
        self.shadow = projection.m2_state(self.view, self.camera_position, environment);
        self
    }

    /// Replaces only the local-light bank while retaining one camera sample.
    #[must_use]
    pub const fn with_local_lights(mut self, local_lights: [M2LocalLightState; 4]) -> Self {
        self.local_lights = local_lights;
        self
    }

    /// Selects the model callback's fog color for its attached particle draws.
    #[must_use]
    pub const fn with_fog_color(mut self, color: Vec3) -> Self {
        self.fog_color = color;
        self
    }

    /// Returns the retained start, end and exponent used by effect submission.
    #[must_use]
    pub const fn fog_parameters(self) -> Vec4 {
        self.fog_parameters
    }

    /// Returns the owner-selected scene fog color before native byte packing.
    #[must_use]
    pub const fn fog_color(self) -> Vec3 {
        self.fog_color
    }

    /// Sets the scene-query fog enable independently of its retained range.
    #[must_use]
    pub const fn with_fog_enabled(mut self, enabled: bool) -> Self {
        self.fog_enabled = enabled;
        self
    }

    /// Reports whether common M2 submission publishes this query's fog bank.
    #[must_use]
    pub const fn fog_enabled(self) -> bool {
        self.fog_enabled
    }

    /// Applies the live build-12340 `specular` CVar to additive M2 stages.
    #[must_use]
    pub const fn with_specular_enabled(mut self, enabled: bool) -> Self {
        self.specular_enabled = enabled;
        self
    }

    /// Serializes without depending on Rust or glam's in-memory representation.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        write_mat4(&mut bytes, &mut offset, self.projection);
        write_vec4(&mut bytes, &mut offset, self.camera_position.extend(1.0));
        write_vec4(&mut bytes, &mut offset, self.ambient_light.extend(0.0));
        write_vec4(&mut bytes, &mut offset, self.diffuse_light.extend(0.0));
        write_vec4(&mut bytes, &mut offset, self.light_direction.extend(0.0));
        let mut fog_parameters = self.fog_parameters;
        // The third lane is private to Solarity's M2 ABI; stock's per-material
        // fog selector lives in M2MaterialUniform instead.
        fog_parameters.z = if self.specular_enabled { 1.0 } else { 0.0 };
        write_vec4(&mut bytes, &mut offset, fog_parameters);
        write_vec4(&mut bytes, &mut offset, self.fog_color.extend(0.0));
        for light in self.local_lights {
            light.write_bytes(&mut bytes, &mut offset);
        }
        self.shadow.write_bytes(&mut bytes, &mut offset);
        write_vec4(&mut bytes, &mut offset, self.view_depth_plane);
        write_mat4(&mut bytes, &mut offset, self.view);
        bytes
    }
}

/// Per-material transforms, color, fog, and alpha threshold block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2MaterialUniform {
    model: Mat4,
    texture_transforms: [Mat4; 2],
    model_view: Mat4,
    mesh_color: Vec4,
    fog_color: Vec4,
    fragment_parameters: Vec4,
    liquid_clip_plane: Vec4,
}

impl M2MaterialUniform {
    /// Returns the owner-selected fog color retained for this mesh submission.
    #[must_use]
    pub fn fog_color(self) -> Vec3 {
        self.fog_color.truncate()
    }

    /// Byte size of the exact std140 material descriptor block.
    pub const BYTE_SIZE: usize = 320;

    /// Returns the complete sampled batch opacity used for shadow admission.
    #[must_use]
    pub fn alpha(self) -> f32 {
        self.mesh_color.w
    }

    /// Creates one immutable material snapshot for a submitted draw.
    #[must_use]
    pub const fn new(
        model: Mat4,
        texture_transforms: [Mat4; 2],
        model_view: Mat4,
        mesh_color: Vec4,
        fog_color: Vec4,
        fragment_parameters: Vec4,
    ) -> Self {
        Self {
            model,
            texture_transforms,
            model_view,
            mesh_color,
            fog_color,
            fragment_parameters,
            liquid_clip_plane: Vec4::W,
        }
    }

    /// Sets the view-space plane for one liquid pass. Nonnegative distances
    /// survive; the default constant plane leaves every vertex unclipped.
    #[must_use]
    pub const fn with_liquid_clip_plane(mut self, plane: Option<Vec4>) -> Self {
        self.liquid_clip_plane = match plane {
            Some(plane) => plane,
            None => Vec4::W,
        };
        self
    }

    /// Serializes without relying on host struct layout or alignment.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        write_mat4(&mut bytes, &mut offset, self.model);
        for transform in self.texture_transforms {
            write_mat4(&mut bytes, &mut offset, transform);
        }
        write_mat4(&mut bytes, &mut offset, self.model_view);
        write_vec4(&mut bytes, &mut offset, self.mesh_color);
        write_vec4(&mut bytes, &mut offset, self.fog_color);
        write_vec4(&mut bytes, &mut offset, self.fragment_parameters);
        write_vec4(&mut bytes, &mut offset, self.liquid_clip_plane);
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

/// Writes one column-major matrix in GLSL's default layout.
fn write_mat4<const N: usize>(bytes: &mut [u8; N], offset: &mut usize, matrix: Mat4) {
    for value in matrix.to_cols_array() {
        write_f32(bytes, offset, value);
    }
}

/// Writes one tightly packed 16-byte std140 vector.
pub(super) fn write_vec4<const N: usize>(bytes: &mut [u8; N], offset: &mut usize, vector: Vec4) {
    for value in vector.to_array() {
        write_f32(bytes, offset, value);
    }
}

/// Writes one little-endian scalar and advances the private fixed-layout cursor.
fn write_f32<const N: usize>(bytes: &mut [u8; N], offset: &mut usize, value: f32) {
    let end = *offset + size_of::<f32>();
    bytes[*offset..end].copy_from_slice(&value.to_le_bytes());
    *offset = end;
}
