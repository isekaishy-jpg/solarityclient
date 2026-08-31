//! Exact 176-byte build-12340 M2 ribbon-emitter decoding.

use glam::Vec3;

use super::{
    M2Sequence, M2Track, array_ref, decode_fixed16, decode_track, decode_vec3, read_f32, read_i8,
    read_i16, read_u8, read_u16, read_u32, validate_array,
};
use crate::{AssetError, AssetPath};

/// One bone-attached ribbon declaration and all of its animation tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct M2RibbonEmitter {
    id: u32,
    bone_index: Option<u32>,
    position: Vec3,
    texture_indices: Vec<u16>,
    material_indices: Vec<u16>,
    color: M2Track<Vec3>,
    alpha: M2Track<f32>,
    height_above: M2Track<f32>,
    height_below: M2Track<f32>,
    edges_per_second: f32,
    edge_lifetime_seconds: f32,
    gravity: f32,
    texture_rows: u16,
    texture_columns: u16,
    texture_slot: M2Track<u16>,
    visibility: M2Track<u8>,
    priority_plane: i16,
    color_index: i8,
    texture_transform_lookup_index: i8,
}

impl M2RibbonEmitter {
    /// Returns the authored emitter identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the owning model bone, or `None` for `0xFFFF_FFFF`.
    #[must_use]
    pub const fn bone_index(&self) -> Option<u32> {
        self.bone_index
    }

    /// Returns the emitter position relative to its owning bone.
    #[must_use]
    pub const fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns texture declarations used by this ribbon in authored order.
    #[must_use]
    pub fn texture_indices(&self) -> &[u16] {
        &self.texture_indices
    }

    /// Returns material declarations used by this ribbon in authored order.
    #[must_use]
    pub fn material_indices(&self) -> &[u16] {
        &self.material_indices
    }

    /// Returns animated linear RGB.
    #[must_use]
    pub const fn color(&self) -> &M2Track<Vec3> {
        &self.color
    }

    /// Returns animated signed-fixed16 opacity.
    #[must_use]
    pub const fn alpha(&self) -> &M2Track<f32> {
        &self.alpha
    }

    /// Returns animated edge height above the emitter center.
    #[must_use]
    pub const fn height_above(&self) -> &M2Track<f32> {
        &self.height_above
    }

    /// Returns animated edge height below the emitter center.
    #[must_use]
    pub const fn height_below(&self) -> &M2Track<f32> {
        &self.height_below
    }

    /// Returns the authored edge sampling rate.
    #[must_use]
    pub const fn edges_per_second(&self) -> f32 {
        self.edges_per_second
    }

    /// Returns the authored edge lifetime in seconds.
    #[must_use]
    pub const fn edge_lifetime_seconds(&self) -> f32 {
        self.edge_lifetime_seconds
    }

    /// Returns the authored downward acceleration.
    #[must_use]
    pub const fn gravity(&self) -> f32 {
        self.gravity
    }

    /// Returns the vertical flipbook cell count.
    #[must_use]
    pub const fn texture_rows(&self) -> u16 {
        self.texture_rows
    }

    /// Returns the horizontal flipbook cell count.
    #[must_use]
    pub const fn texture_columns(&self) -> u16 {
        self.texture_columns
    }

    /// Returns the animated flipbook cell selector.
    #[must_use]
    pub const fn texture_slot(&self) -> &M2Track<u16> {
        &self.texture_slot
    }

    /// Returns the animated byte visibility channel.
    #[must_use]
    pub const fn visibility(&self) -> &M2Track<u8> {
        &self.visibility
    }

    /// Returns the signed scene priority plane.
    #[must_use]
    pub const fn priority_plane(&self) -> i16 {
        self.priority_plane
    }

    /// Returns the optional particle-color palette channel, where `-1` is absent.
    #[must_use]
    pub const fn color_index(&self) -> i8 {
        self.color_index
    }

    /// Returns the optional texture-transform lookup slot, where `-1` is absent.
    #[must_use]
    pub const fn texture_transform_lookup_index(&self) -> i8 {
        self.texture_transform_lookup_index
    }
}

/// Decodes every top-level ribbon and validates its bone ownership.
pub(super) fn decode_ribbons(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
    bone_count: usize,
) -> Result<Vec<M2RibbonEmitter>, AssetError> {
    let array = array_ref(path, bytes, 0x120, "ribbons")?;
    validate_array(path, bytes, array, 176, "ribbons")?;
    let mut ribbons = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 176;
        let field = |name: &str| format!("ribbon {index} {name}");
        let bone_raw = read_u32(path, bytes, offset + 4, &field("bone"))?;
        let bone_index = if bone_raw == u32::MAX {
            None
        } else {
            if bone_raw as usize >= bone_count {
                return Err(crate::model::m2_shared::model_decode(
                    path,
                    format!("ribbon {index} references missing bone {bone_raw}"),
                ));
            }
            Some(bone_raw)
        };
        ribbons.push(M2RibbonEmitter {
            id: read_u32(path, bytes, offset, &field("ID"))?,
            bone_index,
            position: decode_vec3(path, bytes, offset + 8, &field("position"))?,
            texture_indices: decode_u16_array(path, bytes, offset + 20, &field("texture indices"))?,
            material_indices: decode_u16_array(
                path,
                bytes,
                offset + 28,
                &field("material indices"),
            )?,
            color: decode_track(
                path,
                bytes,
                offset + 36,
                &field("color"),
                globals,
                sequences,
                payloads,
                12,
                decode_vec3,
            )?,
            alpha: decode_track(
                path,
                bytes,
                offset + 56,
                &field("alpha"),
                globals,
                sequences,
                payloads,
                2,
                decode_fixed16,
            )?,
            height_above: decode_track(
                path,
                bytes,
                offset + 76,
                &field("height above"),
                globals,
                sequences,
                payloads,
                4,
                read_f32,
            )?,
            height_below: decode_track(
                path,
                bytes,
                offset + 96,
                &field("height below"),
                globals,
                sequences,
                payloads,
                4,
                read_f32,
            )?,
            edges_per_second: read_f32(path, bytes, offset + 116, &field("edge rate"))?,
            edge_lifetime_seconds: read_f32(path, bytes, offset + 120, &field("edge lifetime"))?,
            gravity: read_f32(path, bytes, offset + 124, &field("gravity"))?,
            texture_rows: read_u16(path, bytes, offset + 128, &field("texture rows"))?,
            texture_columns: read_u16(path, bytes, offset + 130, &field("texture columns"))?,
            texture_slot: decode_track(
                path,
                bytes,
                offset + 132,
                &field("texture slot"),
                globals,
                sequences,
                payloads,
                2,
                read_u16,
            )?,
            visibility: decode_track(
                path,
                bytes,
                offset + 152,
                &field("visibility"),
                globals,
                sequences,
                payloads,
                1,
                read_u8,
            )?,
            priority_plane: read_i16(path, bytes, offset + 172, &field("priority plane"))?,
            color_index: read_i8(path, bytes, offset + 174, &field("color index"))?,
            texture_transform_lookup_index: read_i8(
                path,
                bytes,
                offset + 175,
                &field("texture transform lookup"),
            )?,
        });
    }
    Ok(ribbons)
}

/// Copies one validated `M2Array<u16>` without retaining dependency storage.
fn decode_u16_array(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Vec<u16>, AssetError> {
    let array = array_ref(path, bytes, offset, field)?;
    validate_array(path, bytes, array, 2, field)?;
    (0..array.count)
        .map(|index| read_u16(path, bytes, array.offset + index * 2, field))
        .collect()
}
