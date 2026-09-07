//! Monster path messages read by `0073F590` and `0073C8E0`.

use super::MovementPacketError;
use super::reader::MovementReader;
use crate::protocol::MovementSplineFacing;

/// Optional path coordinate parent carried by opcode `0x2AE`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonsterMoveTransport {
    /// Packed transport GUID, including an explicit zero.
    pub guid: u64,
    /// Native signed seat byte.
    pub seat: i8,
}

/// Raw path instructions before the movement owner adjusts the current start.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterMovePath {
    /// Exact flags determining geometry, facing, and effects.
    pub flags: u32,
    /// Authored duration in milliseconds, before Unit_C's speed cap.
    pub duration_ms: u32,
    /// Optional animation tier and its start delay in milliseconds.
    pub animation: Option<(u8, u32)>,
    /// Optional vertical acceleration and start delay in milliseconds.
    pub parabolic: Option<(f32, u32)>,
    /// Authored points, excluding the separately transmitted initial position.
    /// Packed linear offsets have been decoded relative to the midpoint.
    pub points: Vec<[f32; 3]>,
}

/// Complete server-started path, or its short stop form.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterMove {
    /// Unit GUID from the opcode-independent prefix.
    pub guid: u64,
    /// Optional coordinate parent and seat from the transport opcode.
    pub transport: Option<MonsterMoveTransport>,
    /// Byte consumed by `0074B9B0`, setting the unit's 0x40 movement bit.
    pub control_byte: u8,
    /// Server start position; short stops use it as their destination.
    pub start: [f32; 3],
    /// Authoritative path identity.
    pub id: u32,
    /// Native facing type byte; one is the short stop form.
    pub facing_type: u8,
    /// Decoded final-facing union selected by the native type byte.
    pub facing: MovementSplineFacing,
    /// Absent only for the short stop form.
    pub path: Option<MonsterMovePath>,
}

impl MonsterMove {
    /// Decodes either registered monster-move opcode.
    ///
    /// # Errors
    /// Rejects truncated fields, impossible point counts, or trailing bytes.
    pub fn decode(opcode: u16, bytes: &[u8]) -> Result<Option<Self>, MovementPacketError> {
        if !matches!(opcode, 0xdd | 0x2ae) {
            return Ok(None);
        }
        let mut reader = MovementReader::new(bytes);
        let guid = reader.packed_guid()?;
        let transport = if opcode == 0x2ae {
            Some(MonsterMoveTransport {
                guid: reader.packed_guid()?,
                seat: reader.byte()? as i8,
            })
        } else {
            None
        };
        let control_byte = reader.byte()?;
        let start = reader.point()?;
        let id = reader.word()?;
        let facing_type = reader.byte()?;
        let facing = match facing_type {
            2 => MovementSplineFacing::Point(reader.point()?),
            3 => MovementSplineFacing::Target(reader.guid()?),
            4 => MovementSplineFacing::Angle(reader.float()?),
            // The native switch gives all other bytes ordinary direction.
            _ => MovementSplineFacing::Direction,
        };
        let path = if facing_type == 1 {
            None
        } else {
            Some(read_path(&mut reader, start)?)
        };
        reader.finish()?;
        Ok(Some(Self {
            guid,
            transport,
            control_byte,
            start,
            id,
            facing_type,
            facing,
            path,
        }))
    }
}

/// Decode all conditional fields before allocating the bounded point vector.
fn read_path(
    reader: &mut MovementReader<'_>,
    start: [f32; 3],
) -> Result<MonsterMovePath, MovementPacketError> {
    let flags = reader.word()?;
    let animation = if flags & 0x200000 != 0 {
        Some((reader.byte()?, reader.word()?))
    } else {
        None
    };
    let duration_ms = reader.word()?;
    let parabolic = if flags & 0x800 != 0 {
        Some((reader.float()?, reader.word()?))
    } else {
        None
    };
    let count = reader.word()? as usize;
    // Native always reads at least one full point, including when the count is
    // zero. Reject that malformed server contract at the safe Rust boundary.
    if count == 0 {
        return Err(reader.error("path has no points"));
    }
    let smooth = flags & 0x42000 != 0;
    let point_bytes = if smooth { 12 } else { 4 };
    if count - 1 > reader.remaining().saturating_sub(12) / point_bytes || reader.remaining() < 12 {
        return Err(reader.error("path point count exceeds packet"));
    }
    let first = reader.point()?;
    let mut points = Vec::with_capacity(count);
    if smooth {
        points.push(first);
        for _ in 1..count {
            points.push(reader.point()?);
        }
    } else {
        let midpoint = std::array::from_fn::<_, 3, _>(|axis| {
            ((f64::from(start[axis]) + f64::from(first[axis])) * 0.5) as f32
        });
        for _ in 1..count {
            let packed = reader.word()?;
            let offset = [
                (packed << 21) as i32 >> 21,
                (packed << 10) as i32 >> 21,
                packed as i32 >> 22,
            ];
            points.push(std::array::from_fn(|axis| {
                midpoint[axis] - offset[axis] as f32 * 0.25
            }));
        }
        points.push(first);
    }
    Ok(MonsterMovePath {
        flags,
        duration_ms,
        animation,
        parabolic,
        points,
    })
}
