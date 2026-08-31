//! Exact 36-byte build-12340 M2 event and timestamp-only track decoding.

use glam::Vec3;

use super::{
    M2Interpolation, M2Sequence, M2SequenceStorage, array_ref, decode_vec3, read_i16, read_u16,
    read_u32, validate_array,
};
use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// One timestamp-only event timeline selected by animation or a global clock.
#[derive(Clone, Debug, PartialEq)]
pub struct M2EventTrack {
    interpolation: M2Interpolation,
    global_sequence: Option<u16>,
    channels: Vec<Vec<u32>>,
}

impl M2EventTrack {
    /// Returns the interpolation selector retained in the event track header.
    #[must_use]
    pub const fn interpolation(&self) -> M2Interpolation {
        self.interpolation
    }

    /// Returns the global clock index, or `None` for sequence-local time.
    #[must_use]
    pub const fn global_sequence(&self) -> Option<u16> {
        self.global_sequence
    }

    /// Returns trigger timestamps in exact outer-channel order.
    #[must_use]
    pub fn channels(&self) -> &[Vec<u32>] {
        &self.channels
    }
}

/// One model event declaration such as a sound, footstep, or spell trigger.
#[derive(Clone, Debug, PartialEq)]
pub struct M2Event {
    identifier: [u8; 4],
    data: u32,
    bone_index: Option<u32>,
    position: Vec3,
    timeline: M2EventTrack,
}

impl M2Event {
    /// Returns the four identifier bytes in their authored file order.
    #[must_use]
    pub const fn identifier(&self) -> [u8; 4] {
        self.identifier
    }

    /// Returns the event-family-specific payload word.
    #[must_use]
    pub const fn data(&self) -> u32 {
        self.data
    }

    /// Returns the owning bone, accounting for both stock absent sentinels.
    #[must_use]
    pub const fn bone_index(&self) -> Option<u32> {
        self.bone_index
    }

    /// Returns the event position relative to its owning bone.
    #[must_use]
    pub const fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns the timestamp-only trigger timeline.
    #[must_use]
    pub const fn timeline(&self) -> &M2EventTrack {
        &self.timeline
    }
}

/// Decodes every exact WotLK event and validates optional bone ownership.
pub(super) fn decode_events(
    path: &AssetPath,
    bytes: &[u8],
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
    bone_count: usize,
) -> Result<Vec<M2Event>, AssetError> {
    let array = array_ref(path, bytes, 0x100, "events")?;
    validate_array(path, bytes, array, 36, "events")?;
    let mut events = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 36;
        let field = |name: &str| format!("event {index} {name}");
        let bone_raw = read_u32(path, bytes, offset + 8, &field("bone"))?;
        let bone_index = if bone_raw == u32::MAX || bone_raw == u32::from(u16::MAX) {
            None
        } else {
            if bone_raw as usize >= bone_count {
                return Err(model_decode(
                    path,
                    format!("event {index} references missing bone {bone_raw}"),
                ));
            }
            Some(bone_raw)
        };
        events.push(M2Event {
            identifier: read_u32(path, bytes, offset, &field("identifier"))?.to_le_bytes(),
            data: read_u32(path, bytes, offset + 4, &field("data"))?,
            bone_index,
            position: decode_vec3(path, bytes, offset + 12, &field("position"))?,
            timeline: decode_event_track(
                path,
                bytes,
                offset + 24,
                &field("timeline"),
                globals,
                sequences,
                payloads,
            )?,
        });
    }
    Ok(events)
}

/// Resolves one timestamp-only nested track through exact sequence storage.
fn decode_event_track(
    path: &AssetPath,
    model_bytes: &[u8],
    offset: usize,
    field: &str,
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
) -> Result<M2EventTrack, AssetError> {
    let interpolation =
        M2Interpolation::from_raw(path, read_u16(path, model_bytes, offset, field)?, field)?;
    let global_raw = read_i16(path, model_bytes, offset + 2, field)?;
    let global_sequence = if global_raw < 0 {
        None
    } else {
        let value = global_raw as u16;
        if usize::from(value) >= globals.len() {
            return Err(model_decode(
                path,
                format!("{field} global sequence is missing"),
            ));
        }
        Some(value)
    };
    let timestamp_arrays = array_ref(path, model_bytes, offset + 4, field)?;
    validate_array(path, model_bytes, timestamp_arrays, 8, field)?;
    let mut channels = Vec::with_capacity(timestamp_arrays.count);
    for channel_index in 0..timestamp_arrays.count {
        if global_sequence.is_none()
            && sequences
                .get(channel_index)
                .is_some_and(|sequence| sequence.storage == M2SequenceStorage::Alias)
        {
            channels.push(Vec::new());
            continue;
        }
        let payload = if global_sequence.is_none()
            && sequences
                .get(channel_index)
                .is_some_and(|sequence| sequence.storage == M2SequenceStorage::External)
        {
            let Some((payload_path, payload_bytes)) =
                payloads.get(channel_index).and_then(Option::as_ref)
            else {
                channels.push(Vec::new());
                continue;
            };
            (payload_path, payload_bytes.as_slice())
        } else {
            (path, model_bytes)
        };
        let timestamps = array_ref(
            path,
            model_bytes,
            timestamp_arrays.offset + channel_index * 8,
            field,
        )?;
        validate_array(payload.0, payload.1, timestamps, 4, field)?;
        let mut channel = Vec::with_capacity(timestamps.count);
        for timestamp_index in 0..timestamps.count {
            channel.push(read_u32(
                payload.0,
                payload.1,
                timestamps.offset + timestamp_index * 4,
                field,
            )?);
        }
        if !channel.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(model_decode(
                payload.0,
                format!("{field} timestamps are not ordered"),
            ));
        }
        channels.push(channel);
    }
    Ok(M2EventTrack {
        interpolation,
        global_sequence,
        channels,
    })
}
