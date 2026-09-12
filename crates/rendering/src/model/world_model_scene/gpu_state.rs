//! Explicit std140 serialization for MapObj scene and material descriptors.

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{WorldModelBlendMode, WorldModelMaterial};

use crate::{WorldModelFogMode, WorldModelLightingMode, WorldModelSurfacePass};

/// Per-frame light, camera, and fog block shared by visible WMO surfaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelSceneUniform {
    projection: Mat4,
    view: Mat4,
    camera_position: Vec3,
    exterior_ambient: Vec3,
    exterior_direct: Vec3,
    flattened_ambient: Vec3,
    flattened_direct: Vec3,
    light_direction: Vec3,
    fog_parameters: Vec4,
}

impl WorldModelSceneUniform {
    /// Exact std140 descriptor size consumed by the MapObj vertex shader.
    pub const BYTE_SIZE: usize = 176;

    /// Captures one world-light snapshot and derives stock's alternate pair.
    #[must_use]
    pub fn new(
        projection: Mat4,
        view: Mat4,
        camera_position: Vec3,
        exterior_ambient: Vec3,
        exterior_direct: Vec3,
        light_direction: Vec3,
        fog_parameters: Vec4,
    ) -> Self {
        let (flattened_ambient, flattened_direct) =
            flattened_lighting(exterior_ambient, exterior_direct);
        Self {
            projection,
            view,
            camera_position,
            exterior_ambient,
            exterior_direct,
            flattened_ambient,
            flattened_direct,
            light_direction,
            fog_parameters,
        }
    }

    /// Returns the byte-quantized alternate ambient/direct pair.
    #[must_use]
    pub const fn flattened_lighting(self) -> [Vec3; 2] {
        [self.flattened_ambient, self.flattened_direct]
    }

    /// Supplies the same view sample for each material's CPU model-view product.
    pub(crate) const fn view(self) -> Mat4 {
        self.view
    }

    /// Returns the shared model fog interval and exponent.
    #[must_use]
    pub const fn fog_parameters(self) -> Vec4 {
        self.fog_parameters
    }

    /// Serializes without depending on Rust or glam memory layout.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        write_mat4(&mut bytes, &mut offset, self.projection);
        for value in [
            self.camera_position,
            self.exterior_ambient,
            self.exterior_direct,
            self.flattened_ambient,
            self.flattened_direct,
            self.light_direction,
        ] {
            write_vec4(&mut bytes, &mut offset, value.extend(0.0));
        }
        write_vec4(&mut bytes, &mut offset, self.fog_parameters);
        bytes
    }
}

/// Per-pass placement, root light, additive color, fog, and behavior block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelMaterialUniform {
    model: Mat4,
    root_ambient: Vec3,
    additive_color: Vec3,
    fog_color: Vec3,
    alpha_reference: f32,
    behavior: [u32; 4],
}

impl WorldModelMaterialUniform {
    /// ShadowMapSL reads only the model matrix from the shared material block.
    pub(crate) const fn shadow(model: Mat4) -> Self {
        Self {
            model,
            root_ambient: Vec3::ZERO,
            additive_color: Vec3::ZERO,
            fog_color: Vec3::ZERO,
            alpha_reference: 0.,
            behavior: [0; 4],
        }
    }

    /// Exact std140 descriptor size consumed by both MapObj stages.
    pub const BYTE_SIZE: usize = 208;

    /// Creates one pass snapshot from decoded MOMT and live environment state.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        model: Mat4,
        root_ambient_bgra: [u8; 4],
        material: &WorldModelMaterial,
        pass: WorldModelSurfacePass,
        environment_emissive: f32,
        fog_color: Vec3,
    ) -> Self {
        let state = pass.material();
        let blend_mode = state.blend().mode();
        Self {
            model,
            root_ambient: bgra_rgb(root_ambient_bgra),
            additive_color: additive_color(material, environment_emissive),
            fog_color,
            alpha_reference: state.alpha_reference(),
            behavior: [
                lighting_code(pass.lighting()),
                u32::from(state.is_unlit()),
                fog_code(state.fog_mode()),
                u32::from(matches!(
                    blend_mode,
                    WorldModelBlendMode::Opaque | WorldModelBlendMode::AlphaKey
                )),
            ],
        }
    }

    /// Returns the post-DayNight, half-intensity MOMT additive color.
    #[must_use]
    pub const fn additive_color(self) -> Vec3 {
        self.additive_color
    }

    /// Returns shader behavior words in lighting/unlit/fog/opaque order.
    #[must_use]
    pub const fn behavior(self) -> [u32; 4] {
        self.behavior
    }

    /// Serializes the world transform for lighting and the CPU-composed
    /// model-view transform for stock's separate projection stage. The latter
    /// avoids adding a large world origin to every vertex before subtracting it.
    #[must_use]
    pub fn to_bytes(self, view: Mat4) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        write_mat4(&mut bytes, &mut offset, self.model);
        for value in [self.root_ambient, self.additive_color, self.fog_color] {
            write_vec4(&mut bytes, &mut offset, value.extend(0.0));
        }
        write_vec4(
            &mut bytes,
            &mut offset,
            Vec4::new(self.alpha_reference, 0.0, 0.0, 0.0),
        );
        for value in self.behavior {
            write_u32(&mut bytes, &mut offset, value);
        }
        write_mat4(&mut bytes, &mut offset, view * self.model);
        bytes
    }
}

/// Replays `0x007EE750` using byte channels and signed arithmetic shift.
fn flattened_lighting(ambient: Vec3, direct: Vec3) -> (Vec3, Vec3) {
    let channel = |ambient: f32, direct: f32| {
        let ambient = color_byte(ambient);
        let direct = color_byte(direct);
        let delta = i32::from(ambient) - i32::from(direct);
        let half_delta = if delta >= 0 {
            delta / 2
        } else {
            -((-delta + 1) / 2)
        };
        let midpoint = (i32::from(direct) + half_delta) as u8;
        [midpoint.saturating_add(16), midpoint]
    };
    let red = channel(ambient.x, direct.x);
    let green = channel(ambient.y, direct.y);
    let blue = channel(ambient.z, direct.z);
    const SCALE: f32 = 1.0 / 255.0;
    (
        Vec3::new(red[0] as f32, green[0] as f32, blue[0] as f32) * SCALE,
        Vec3::new(red[1] as f32, green[1] as f32, blue[1] as f32) * SCALE,
    )
}

fn color_byte(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Replays the x87-rounded `0x007A8520` update and submitter's second shift.
fn additive_color(material: &WorldModelMaterial, environment_emissive: f32) -> Vec3 {
    if material.flags() & 0x10 == 0 {
        return Vec3::ZERO;
    }
    let factor = if environment_emissive.is_finite() {
        environment_emissive.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let scaled = factor * 255.0 - 0.5;
    let lower = scaled.floor() as i32;
    let fraction = scaled - lower as f32;
    let nearest_even = if fraction > 0.5 || (fraction == 0.5 && lower & 1 != 0) {
        lower + 1
    } else {
        lower
    };
    let multiplier = nearest_even.clamp(0, 255) as u32;
    let color = material.emissive_color();
    let channel = |byte: u32| (((byte * multiplier) >> 8) >> 1) as f32 / 255.0;
    Vec3::new(
        channel((color >> 16) & 0xff),
        channel((color >> 8) & 0xff),
        channel(color & 0xff),
    )
}

const fn bgra_rgb(color: [u8; 4]) -> Vec3 {
    const SCALE: f32 = 1.0 / 255.0;
    Vec3::new(
        color[2] as f32 * SCALE,
        color[1] as f32 * SCALE,
        color[0] as f32 * SCALE,
    )
}

const fn lighting_code(mode: WorldModelLightingMode) -> u32 {
    match mode {
        WorldModelLightingMode::Authored => 0,
        WorldModelLightingMode::Exterior => 1,
        WorldModelLightingMode::FlattenedExterior => 2,
        WorldModelLightingMode::RootAmbient => 3,
    }
}

const fn fog_code(mode: WorldModelFogMode) -> u32 {
    match mode {
        WorldModelFogMode::Disabled => 0,
        WorldModelFogMode::SceneColor => 1,
        WorldModelFogMode::Black => 2,
        WorldModelFogMode::White => 3,
        WorldModelFogMode::HalfWhite => 4,
    }
}

fn write_mat4<const N: usize>(bytes: &mut [u8; N], offset: &mut usize, matrix: Mat4) {
    for value in matrix.to_cols_array() {
        write_f32(bytes, offset, value);
    }
}

fn write_vec4<const N: usize>(bytes: &mut [u8; N], offset: &mut usize, vector: Vec4) {
    for value in vector.to_array() {
        write_f32(bytes, offset, value);
    }
}

fn write_f32<const N: usize>(bytes: &mut [u8; N], offset: &mut usize, value: f32) {
    write_u32(bytes, offset, value.to_bits());
}

fn write_u32<const N: usize>(bytes: &mut [u8; N], offset: &mut usize, value: u32) {
    let end = *offset + size_of::<u32>();
    bytes[*offset..end].copy_from_slice(&value.to_le_bytes());
    *offset = end;
}
