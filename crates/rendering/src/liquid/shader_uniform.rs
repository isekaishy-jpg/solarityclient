//! Explicit liquid draw ABI for the original 8A32F0/8A38B0 shader inputs.

use glam::{Mat4, Vec3, Vec4};

/// One of the first three point lights retained by native 8A38B0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidPointLight {
    position: Vec3,
    color: Vec3,
    attenuation: Vec3,
}

impl LiquidPointLight {
    /// Retains view-space position, normalized RGB, and constant/linear/quadratic attenuation.
    #[must_use]
    pub const fn new(position: Vec3, color: Vec3, attenuation: Vec3) -> Self {
        Self {
            position,
            color,
            attenuation,
        }
    }
}

/// Directional, ambient, specular, and up to three local liquid light sources.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidLighting {
    direction: Vec3,
    ambient: Vec3,
    diffuse: Vec3,
    specular: Vec3,
    points: [LiquidPointLight; 3],
    point_count: u32,
}

impl LiquidLighting {
    /// Retains native view-space light-ray direction and its three color terms.
    #[must_use]
    pub const fn new(direction: Vec3, ambient: Vec3, diffuse: Vec3, specular: Vec3) -> Self {
        Self {
            direction,
            ambient,
            diffuse,
            specular,
            points: [LiquidPointLight::new(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO); 3],
            point_count: 0,
        }
    }

    /// Selects the first three lights in the original query order, as 8A38B0 does.
    #[must_use]
    pub fn with_point_lights(mut self, lights: &[LiquidPointLight]) -> Self {
        let count = lights.len().min(self.points.len());
        self.points[..count].copy_from_slice(&lights[..count]);
        self.point_count = count as u32;
        self
    }

    /// Replays scene directional collection after the liquid provider's sun.
    /// Native 834F60 sums ambient but 834DC0 retains the last diffuse/direction;
    /// water's 8A38B0 upload does not run M2's merged-sun finalizer.
    #[must_use]
    pub fn with_scene_directional_lights(
        mut self,
        view: Mat4,
        lights: &[crate::M2DirectionalLight],
    ) -> Self {
        for light in lights {
            self.ambient += light.ambient();
            self.diffuse = light.diffuse();
            self.direction = view.transform_vector3(light.direction());
        }
        self
    }
}

/// Native vertex fog coefficients and the post-pixel-shader fog color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidFog {
    coefficients: Vec3,
    color: Vec3,
}

impl LiquidFog {
    /// Selects another palette bank while retaining the shared fog coefficients.
    #[must_use]
    pub const fn with_color(self, color: Vec3) -> Self {
        Self { color, ..self }
    }

    /// Retains `(view_z_multiplier, offset, exponent)` from native register c4.
    ///
    /// Visibility is `min(max(view_z * multiplier + offset, 0)^exponent, 1)`.
    /// Native 8A38B0 writes `(0, 1, 1)` when the fog provider disables fog.
    #[must_use]
    pub const fn new(coefficients: Vec3, color: Vec3) -> Self {
        Self {
            coefficients,
            color,
        }
    }
}

/// One complete liquid draw, serialized independently of host and glam layouts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidShaderUniform {
    projection: Mat4,
    model_view: Mat4,
    surface_transform: Mat4,
    depth_transform: Mat4,
    lighting: LiquidLighting,
    fog: LiquidFog,
}

impl LiquidShaderUniform {
    /// Exact std140 byte extent, including the three native point-light slots.
    pub const BYTE_SIZE: usize = 512;

    /// Captures the matrix, material, lighting, and fog sample for one draw.
    #[must_use]
    pub const fn new(
        projection: Mat4,
        model_view: Mat4,
        surface_transform: Mat4,
        depth_transform: Mat4,
        lighting: LiquidLighting,
        fog: LiquidFog,
    ) -> Self {
        Self {
            projection,
            model_view,
            surface_transform,
            depth_transform,
            lighting,
            fog,
        }
    }

    /// Serializes the exact liquid vertex/fragment descriptor block.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0; Self::BYTE_SIZE];
        let mut cursor = 0;
        for matrix in [
            self.projection,
            self.model_view,
            self.surface_transform,
            self.depth_transform,
        ] {
            for column in matrix.to_cols_array_2d() {
                write_vector(&mut bytes, &mut cursor, Vec4::from_array(column));
            }
        }
        for vector in [
            self.fog.coefficients.extend(0.0),
            self.lighting.direction.extend(1.0),
            self.lighting.ambient.extend(1.0),
            self.lighting.diffuse.extend(1.0),
            // Native 8A38B0 uses the constant at 9E8CF8, not a material parameter.
            self.lighting.specular.extend(6.0),
            self.fog.color.extend(1.0),
        ] {
            write_vector(&mut bytes, &mut cursor, vector);
        }
        for point in self.lighting.points {
            for vector in [
                point.position.extend(1.0),
                point.color.extend(1.0),
                point.attenuation.extend(0.0),
            ] {
                write_vector(&mut bytes, &mut cursor, vector);
            }
        }
        bytes[cursor..cursor + 4].copy_from_slice(&self.lighting.point_count.to_le_bytes());
        debug_assert_eq!(cursor + 16, Self::BYTE_SIZE);
        bytes
    }
}

/// Advances the std140 cursor by one explicitly serialized four-float slot.
fn write_vector(
    bytes: &mut [u8; LiquidShaderUniform::BYTE_SIZE],
    cursor: &mut usize,
    vector: Vec4,
) {
    for value in vector.to_array() {
        bytes[*cursor..*cursor + 4].copy_from_slice(&value.to_le_bytes());
        *cursor += 4;
    }
}
