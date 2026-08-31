//! Exact 40-byte build-12340 M2 attachment and lookup decoding.

use glam::Vec3;

use super::{
    M2Sequence, M2Track, array_ref, decode_track, decode_vec3, read_u8, read_u16, read_u32,
    validate_array,
};
use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// One bone-relative attachment point and its animated enable channel.
#[derive(Clone, Debug, PartialEq)]
pub struct M2Attachment {
    id: u32,
    bone_index: u16,
    unknown: u16,
    position: Vec3,
    enabled: M2Track<u8>,
}

impl M2Attachment {
    /// Returns the stock attachment identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the model bone that owns this attachment.
    #[must_use]
    pub const fn bone_index(&self) -> u16 {
        self.bone_index
    }

    /// Returns the preserved unknown word between bone and position.
    #[must_use]
    pub const fn unknown(&self) -> u16 {
        self.unknown
    }

    /// Returns the stable authored model-space position.
    ///
    /// This deliberately excludes the current bone pose. Build 12340 reads
    /// the authored Breath attachment position for player-camera height so
    /// the orbit pivot does not bob with the walk animation.
    #[must_use]
    pub const fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns the byte-valued animated enable channel.
    #[must_use]
    pub const fn enabled(&self) -> &M2Track<u8> {
        &self.enabled
    }
}

/// Decodes exact WotLK attachments and their authoritative semantic lookup.
pub(super) fn decode_attachments(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
    bone_count: usize,
) -> Result<(Vec<M2Attachment>, Vec<u16>), AssetError> {
    let array = array_ref(path, bytes, 0xf0, "attachments")?;
    validate_array(path, bytes, array, 40, "attachments")?;
    let mut attachments = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 40;
        let field = |name: &str| format!("attachment {index} {name}");
        let bone_index = read_u16(path, bytes, offset + 4, &field("bone"))?;
        if usize::from(bone_index) >= bone_count {
            return Err(model_decode(
                path,
                format!("attachment {index} references missing bone {bone_index}"),
            ));
        }
        attachments.push(M2Attachment {
            id: read_u32(path, bytes, offset, &field("ID"))?,
            bone_index,
            unknown: read_u16(path, bytes, offset + 6, &field("unknown"))?,
            position: decode_vec3(path, bytes, offset + 8, &field("position"))?,
            enabled: decode_track(
                path,
                bytes,
                offset + 20,
                &field("enabled"),
                globals,
                sequences,
                payloads,
                1,
                read_u8,
            )?,
        });
    }
    let lookup = decode_attachment_lookup(path, bytes, attachments.len())?;
    Ok((attachments, lookup))
}

/// Preserves `0xFFFF` lookup holes and rejects every other missing record.
fn decode_attachment_lookup(
    path: &AssetPath,
    bytes: &[u8],
    attachment_count: usize,
) -> Result<Vec<u16>, AssetError> {
    let array = array_ref(path, bytes, 0xf8, "attachment lookup")?;
    validate_array(path, bytes, array, 2, "attachment lookup")?;
    let mut lookup = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let value = read_u16(path, bytes, array.offset + index * 2, "attachment lookup")?;
        if value != u16::MAX && usize::from(value) >= attachment_count {
            return Err(model_decode(
                path,
                format!("attachment lookup {index} references missing attachment {value}"),
            ));
        }
        lookup.push(value);
    }
    Ok(lookup)
}
