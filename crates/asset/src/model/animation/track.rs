//! Property-typed M2 key storage and nested sequence-channel decoding.

use super::{
    M2Sequence, M2SequenceStorage, array_ref, read_i16, read_u16, read_u32, validate_array,
};
use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// Stock interpolation selector authored by one M2 track.
/// Camera spline keys use the cubic operations; ordinary continuous properties
/// interpolate linearly for every nonzero selector. Storage follows the key type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2Interpolation {
    /// Hold the preceding key.
    Step,
    /// Interpolate directly between adjacent values.
    Linear,
    /// Use the incoming and outgoing Bezier control points stored with each key.
    Bezier,
    /// Use the incoming and outgoing Hermite tangents stored with each key.
    Hermite,
}

impl M2Interpolation {
    /// Converts the version-264 selector without substituting unknown values.
    pub(super) fn from_raw(path: &AssetPath, value: u16, field: &str) -> Result<Self, AssetError> {
        match value {
            0 => Ok(Self::Step),
            1 => Ok(Self::Linear),
            2 => Ok(Self::Bezier),
            3 => Ok(Self::Hermite),
            _ => Err(model_decode(
                path,
                format!("{field} has unsupported interpolation {value}"),
            )),
        }
    }
}

/// One timestamp/value channel selected by a sequence or global clock.
#[derive(Clone, Debug, PartialEq)]
pub struct M2TrackChannel<T> {
    timestamps_ms: Vec<u32>,
    values: Vec<T>,
}

impl<T> M2TrackChannel<T> {
    /// Returns ordered key timestamps in milliseconds.
    #[must_use]
    pub fn timestamps_ms(&self) -> &[u32] {
        &self.timestamps_ms
    }

    /// Returns one typed value per timestamp, including complete spline keys.
    #[must_use]
    pub fn values(&self) -> &[T] {
        &self.values
    }
}

/// Every per-sequence channel for one animated M2 property.
#[derive(Clone, Debug, PartialEq)]
pub struct M2Track<T> {
    interpolation: M2Interpolation,
    global_sequence: Option<u16>,
    channels: Vec<M2TrackChannel<T>>,
}

impl<T> M2Track<T> {
    /// Returns the authored interpolation operation.
    #[must_use]
    pub const fn interpolation(&self) -> M2Interpolation {
        self.interpolation
    }

    /// Returns the global-sequence clock index, or `None` for animation time.
    #[must_use]
    pub const fn global_sequence(&self) -> Option<u16> {
        self.global_sequence
    }

    /// Returns channels in their exact outer-array order.
    #[must_use]
    pub fn channels(&self) -> &[M2TrackChannel<T>] {
        &self.channels
    }
}

/// A camera key always stores a value and two controls, even for step/linear.
///
/// `0x0082B460` advances vector keys by 36 bytes; `0x0082B8A0` advances scalar
/// keys by 12. Ordinary tracks contain their scalar/vector type directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2SplineKey<T> {
    value: T,
    incoming: T,
    outgoing: T,
}

impl<T> M2SplineKey<T> {
    /// Retains the complete authored triplet in native storage order.
    pub(super) const fn new(value: T, incoming: T, outgoing: T) -> Self {
        Self {
            value,
            incoming,
            outgoing,
        }
    }

    /// Returns the authored value at this timestamp.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Returns the incoming Bezier control point or Hermite tangent.
    #[must_use]
    pub const fn incoming(&self) -> &T {
        &self.incoming
    }

    /// Returns the outgoing Bezier control point or Hermite tangent.
    #[must_use]
    pub const fn outgoing(&self) -> &T {
        &self.outgoing
    }
}

/// Expands one WotLK nested track using each sequence's actual data source.
#[allow(clippy::too_many_arguments)]
pub(super) fn decode_track<T>(
    path: &AssetPath,
    model_bytes: &[u8],
    offset: usize,
    field: &str,
    globals: &[u32],
    sequences: &[M2Sequence],
    payloads: &[Option<(AssetPath, Vec<u8>)>],
    value_size: usize,
    decode_value: fn(&AssetPath, &[u8], usize, &str) -> Result<T, AssetError>,
) -> Result<M2Track<T>, AssetError> {
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
    let value_arrays = array_ref(path, model_bytes, offset + 12, field)?;
    validate_array(path, model_bytes, timestamp_arrays, 8, field)?;
    validate_array(path, model_bytes, value_arrays, 8, field)?;
    if timestamp_arrays.count != value_arrays.count {
        return Err(model_decode(path, format!("{field} channel counts differ")));
    }
    let mut channels = Vec::with_capacity(timestamp_arrays.count);
    for channel_index in 0..timestamp_arrays.count {
        if global_sequence.is_none()
            && sequences
                .get(channel_index)
                .is_some_and(|value| value.storage == M2SequenceStorage::Alias)
        {
            channels.push(empty_channel());
            continue;
        }
        let payload = if global_sequence.is_none()
            && sequences
                .get(channel_index)
                .is_some_and(|value| value.storage == M2SequenceStorage::External)
        {
            let Some((payload_path, payload_bytes)) =
                payloads.get(channel_index).and_then(Option::as_ref)
            else {
                channels.push(empty_channel());
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
        let values = array_ref(
            path,
            model_bytes,
            value_arrays.offset + channel_index * 8,
            field,
        )?;
        validate_array(payload.0, payload.1, timestamps, 4, field)?;
        if values.count != timestamps.count {
            return Err(model_decode(
                payload.0,
                format!("{field} key counts differ"),
            ));
        }
        // Native validators choose the element width from the property type,
        // independently of interpolation (e.g. 0x00837130: 36-byte camera
        // vectors; 0x008371C0: 12-byte ordinary vectors or camera scalars).
        let stored_count = timestamps.count;
        validate_array(payload.0, payload.1, values, value_size, field)?;
        let mut timestamps_ms = Vec::with_capacity(timestamps.count);
        for index in 0..timestamps.count {
            timestamps_ms.push(read_u32(
                payload.0,
                payload.1,
                timestamps.offset + index * 4,
                field,
            )?);
        }
        if !timestamps_ms.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(model_decode(
                payload.0,
                format!("{field} timestamps are not ordered"),
            ));
        }
        let mut decoded_values = Vec::with_capacity(stored_count);
        for index in 0..stored_count {
            decoded_values.push(decode_value(
                payload.0,
                payload.1,
                values.offset + index * value_size,
                field,
            )?);
        }
        channels.push(M2TrackChannel {
            timestamps_ms,
            values: decoded_values,
        });
    }
    Ok(M2Track {
        interpolation,
        global_sequence,
        channels,
    })
}

/// Builds the explicit empty slot retained for aliases and unavailable payloads.
fn empty_channel<T>() -> M2TrackChannel<T> {
    M2TrackChannel {
        timestamps_ms: Vec::new(),
        values: Vec::new(),
    }
}
