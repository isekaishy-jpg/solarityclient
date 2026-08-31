//! Exact 156-byte build-12340 M2 light decoding.

use glam::Vec3;

use super::{
    M2Sequence, M2Track, array_ref, decode_track, decode_vec3, read_f32, read_i16, read_u8,
    read_u16, validate_array,
};
use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// The two light families admitted by the build-12340 model loader.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2LightKind {
    /// A direction derived from the owning bone transform.
    Directional,
    /// A position transformed by the owning bone.
    Point,
}

impl M2LightKind {
    /// Narrows the exact WotLK selector without accepting later light types.
    fn from_raw(path: &AssetPath, index: usize, value: u16) -> Result<Self, AssetError> {
        match value {
            0 => Ok(Self::Directional),
            1 => Ok(Self::Point),
            _ => Err(model_decode(
                path,
                format!("light {index} has unsupported type {value}"),
            )),
        }
    }
}

/// One bone-relative model light and all seven of its animation tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct M2Light {
    kind: M2LightKind,
    bone_index: Option<u16>,
    position: Vec3,
    ambient_color: M2Track<Vec3>,
    ambient_intensity: M2Track<f32>,
    diffuse_color: M2Track<Vec3>,
    diffuse_intensity: M2Track<f32>,
    attenuation_start: M2Track<f32>,
    attenuation_end: M2Track<f32>,
    visibility: M2Track<u8>,
}

impl M2Light {
    /// Returns whether the record emits a directional or point light.
    #[must_use]
    pub const fn kind(&self) -> M2LightKind {
        self.kind
    }

    /// Returns the owning bone, or `None` for the signed `-1` sentinel.
    #[must_use]
    pub const fn bone_index(&self) -> Option<u16> {
        self.bone_index
    }

    /// Returns the authored position relative to the owning bone.
    #[must_use]
    pub const fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns animated linear ambient RGB.
    #[must_use]
    pub const fn ambient_color(&self) -> &M2Track<Vec3> {
        &self.ambient_color
    }

    /// Returns the animated ambient-color multiplier.
    #[must_use]
    pub const fn ambient_intensity(&self) -> &M2Track<f32> {
        &self.ambient_intensity
    }

    /// Returns animated linear diffuse RGB.
    #[must_use]
    pub const fn diffuse_color(&self) -> &M2Track<Vec3> {
        &self.diffuse_color
    }

    /// Returns the animated diffuse-color multiplier.
    #[must_use]
    pub const fn diffuse_intensity(&self) -> &M2Track<f32> {
        &self.diffuse_intensity
    }

    /// Returns the animated distance where point-light attenuation begins.
    #[must_use]
    pub const fn attenuation_start(&self) -> &M2Track<f32> {
        &self.attenuation_start
    }

    /// Returns the animated distance where point-light attenuation ends.
    #[must_use]
    pub const fn attenuation_end(&self) -> &M2Track<f32> {
        &self.attenuation_end
    }

    /// Returns the byte-valued animated visibility channel.
    #[must_use]
    pub const fn visibility(&self) -> &M2Track<u8> {
        &self.visibility
    }
}

/// Decodes all exact WotLK model lights and validates their bone references.
pub(super) fn decode_lights(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
    bone_count: usize,
) -> Result<Vec<M2Light>, AssetError> {
    let array = array_ref(path, bytes, 0x108, "lights")?;
    validate_array(path, bytes, array, 156, "lights")?;
    let mut lights = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 156;
        let field = |name: &str| format!("light {index} {name}");
        let bone_raw = read_i16(path, bytes, offset + 2, &field("bone"))?;
        let bone_index = if bone_raw == -1 {
            None
        } else if bone_raw < -1 || bone_raw as usize >= bone_count {
            return Err(model_decode(
                path,
                format!("light {index} references missing bone {bone_raw}"),
            ));
        } else {
            Some(bone_raw as u16)
        };
        lights.push(M2Light {
            kind: M2LightKind::from_raw(
                path,
                index,
                read_u16(path, bytes, offset, &field("type"))?,
            )?,
            bone_index,
            position: decode_vec3(path, bytes, offset + 4, &field("position"))?,
            ambient_color: decode_track(
                path,
                bytes,
                offset + 0x10,
                &field("ambient color"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            ambient_intensity: float_track(
                path,
                bytes,
                offset + 0x24,
                &field("ambient intensity"),
                globals,
                sequences,
                payloads,
            )?,
            diffuse_color: decode_track(
                path,
                bytes,
                offset + 0x38,
                &field("diffuse color"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            diffuse_intensity: float_track(
                path,
                bytes,
                offset + 0x4c,
                &field("diffuse intensity"),
                globals,
                sequences,
                payloads,
            )?,
            attenuation_start: float_track(
                path,
                bytes,
                offset + 0x60,
                &field("attenuation start"),
                globals,
                sequences,
                payloads,
            )?,
            attenuation_end: float_track(
                path,
                bytes,
                offset + 0x74,
                &field("attenuation end"),
                globals,
                sequences,
                payloads,
            )?,
            visibility: decode_track(
                path,
                bytes,
                offset + 0x88,
                &field("visibility"),
                globals,
                sequences,
                payloads,
                1,
                read_u8,
            )?,
        });
    }
    Ok(lights)
}

/// Decodes one conventional float-valued model-light track.
fn float_track(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
) -> Result<M2Track<f32>, AssetError> {
    decode_track(
        path, bytes, offset, field, globals, sequences, payloads, 4, read_f32,
    )
}
