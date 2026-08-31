//! Exact 100-byte build-12340 M2 camera and signed lookup decoding.

use glam::Vec3;

use super::{
    M2Sequence, M2Track, array_ref, decode_track, decode_vec3, read_f32, read_i16, read_i32,
    validate_array,
};
use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// One model-authored camera used by portraits, presentation, or cinematics.
#[derive(Clone, Debug, PartialEq)]
pub struct M2Camera {
    kind: i32,
    field_of_view_radians: f32,
    far_clip: f32,
    near_clip: f32,
    position: M2Track<Vec3>,
    position_base: Vec3,
    target_position: M2Track<Vec3>,
    target_position_base: Vec3,
    roll_radians: M2Track<f32>,
}

impl M2Camera {
    /// Returns the signed stock camera-role selector.
    #[must_use]
    pub const fn kind(&self) -> i32 {
        self.kind
    }

    /// Returns the authored vertical field of view in radians.
    #[must_use]
    pub const fn field_of_view_radians(&self) -> f32 {
        self.field_of_view_radians
    }

    /// Returns the authored far clipping distance.
    #[must_use]
    pub const fn far_clip(&self) -> f32 {
        self.far_clip
    }

    /// Returns the authored positive near clipping distance.
    #[must_use]
    pub const fn near_clip(&self) -> f32 {
        self.near_clip
    }

    /// Returns animated offsets from the position base.
    #[must_use]
    pub const fn position(&self) -> &M2Track<Vec3> {
        &self.position
    }

    /// Returns the authored base camera position.
    #[must_use]
    pub const fn position_base(&self) -> Vec3 {
        self.position_base
    }

    /// Returns animated offsets from the target-position base.
    #[must_use]
    pub const fn target_position(&self) -> &M2Track<Vec3> {
        &self.target_position
    }

    /// Returns the authored base camera target.
    #[must_use]
    pub const fn target_position_base(&self) -> Vec3 {
        self.target_position_base
    }

    /// Returns animated view-axis roll in radians.
    #[must_use]
    pub const fn roll_radians(&self) -> &M2Track<f32> {
        &self.roll_radians
    }
}

/// Decodes the exact camera records and their signed semantic lookup table.
pub(super) fn decode_cameras(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
) -> Result<(Vec<M2Camera>, Vec<Option<u16>>), AssetError> {
    let array = array_ref(path, bytes, 0x110, "cameras")?;
    validate_array(path, bytes, array, 100, "cameras")?;
    let mut cameras = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 100;
        let field = |name: &str| format!("camera {index} {name}");
        let field_of_view_radians = read_f32(path, bytes, offset + 4, &field("field of view"))?;
        let far_clip = read_f32(path, bytes, offset + 8, &field("far clip"))?;
        let near_clip = read_f32(path, bytes, offset + 12, &field("near clip"))?;
        if field_of_view_radians <= 0.0 || near_clip <= 0.0 || far_clip <= near_clip {
            return Err(model_decode(
                path,
                format!("camera {index} has invalid field of view or clip planes"),
            ));
        }
        cameras.push(M2Camera {
            kind: read_i32(path, bytes, offset, &field("type"))?,
            field_of_view_radians,
            far_clip,
            near_clip,
            position: decode_track(
                path,
                bytes,
                offset + 16,
                &field("position"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            position_base: decode_vec3(path, bytes, offset + 36, &field("position base"))?,
            target_position: decode_track(
                path,
                bytes,
                offset + 48,
                &field("target position"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            target_position_base: decode_vec3(
                path,
                bytes,
                offset + 68,
                &field("target position base"),
            )?,
            roll_radians: decode_track(
                path,
                bytes,
                offset + 80,
                &field("roll"),
                globals,
                sequences,
                payloads,
                4,
                read_f32,
            )?,
        });
    }
    let lookup = decode_camera_lookup(path, bytes, cameras.len())?;
    Ok((cameras, lookup))
}

/// Decodes `-1` as an absent semantic camera and validates every other slot.
fn decode_camera_lookup(
    path: &AssetPath,
    bytes: &[u8],
    camera_count: usize,
) -> Result<Vec<Option<u16>>, AssetError> {
    let array = array_ref(path, bytes, 0x118, "camera lookup")?;
    validate_array(path, bytes, array, 2, "camera lookup")?;
    let mut lookup = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let value = read_i16(path, bytes, array.offset + index * 2, "camera lookup")?;
        if value == -1 {
            lookup.push(None);
        } else if value < -1 || value as usize >= camera_count {
            return Err(model_decode(
                path,
                format!("camera lookup {index} references missing camera {value}"),
            ));
        } else {
            lookup.push(Some(value as u16));
        }
    }
    Ok(lookup)
}
