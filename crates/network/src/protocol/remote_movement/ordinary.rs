//! Packed GUID and MovementInfo admitted by native `00741B60` / `00740D30`.

#[cfg(test)]
#[path = "../../../tests/protocol/remote_movement.rs"]
mod tests;

use super::reader::{MovementPacketError, MovementReader};
use crate::protocol::{
    ObjectMovementContext, ObjectMovementFall, ObjectMovementTransport, WorldMovementKind,
};

/// An ordinary remote command. Speeds and path data belong to separate packets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteMovement {
    /// Registered movement event.
    pub kind: WorldMovementKind,
    /// Subject resolved against the current world lifetime.
    pub guid: u64,
    /// Exact 32-bit primary and 16-bit secondary wire flags.
    pub flags: u64,
    /// Sender world position in yards.
    pub position: [f32; 3],
    /// Sender facing in radians.
    pub orientation: f32,
    /// Conditional attachment, pitch, and launch fields.
    pub context: ObjectMovementContext,
}

impl RemoteMovement {
    /// Returns `None` for another packet envelope before inspecting its body.
    pub(crate) fn decode(opcode: u16, bytes: &[u8]) -> Result<Option<Self>, MovementPacketError> {
        let kind = match opcode {
            0xb5 => WorldMovementKind::StartForward,
            0xb6 => WorldMovementKind::StartBackward,
            0xb7 => WorldMovementKind::Stop,
            0xb8 => WorldMovementKind::StartStrafeLeft,
            0xb9 => WorldMovementKind::StartStrafeRight,
            0xba => WorldMovementKind::StopStrafe,
            0xbb => WorldMovementKind::Jump,
            0xbc => WorldMovementKind::StartTurnLeft,
            0xbd => WorldMovementKind::StartTurnRight,
            0xbe => WorldMovementKind::StopTurn,
            0xbf => WorldMovementKind::StartPitchUp,
            0xc0 => WorldMovementKind::StartPitchDown,
            0xc1 => WorldMovementKind::StopPitch,
            0xc2 => WorldMovementKind::SetRunMode,
            0xc3 => WorldMovementKind::SetWalkMode,
            0xc9 => WorldMovementKind::FallLand,
            0xca => WorldMovementKind::StartSwim,
            0xcb => WorldMovementKind::StopSwim,
            0xda => WorldMovementKind::SetFacing,
            0xdb => WorldMovementKind::SetPitch,
            0xee => WorldMovementKind::Heartbeat,
            0x359 => WorldMovementKind::StartAscend,
            0x35a => WorldMovementKind::StopAscend,
            0x3a7 => WorldMovementKind::StartDescend,
            _ => return Ok(None),
        };
        let mut reader = MovementReader::new(bytes);
        let guid = reader.packed_guid()?;
        let flags = u64::from(reader.word()?) | (u64::from(reader.short()?) << 32);
        let timestamp_ms = reader.word()?;
        let position = reader.point()?;
        let orientation = reader.float()?;
        let transport = if flags & 0x200 != 0 {
            Some(ObjectMovementTransport {
                guid: reader.packed_guid()?,
                position: reader.point()?,
                orientation: reader.float()?,
                time_ms: reader.word()?,
                seat: reader.byte()? as i8,
                interpolated_time_ms: if flags & 0x0400_0000_0000 != 0 {
                    Some(reader.word()?)
                } else {
                    None
                },
            })
        } else {
            None
        };
        let pitch_radians = if flags & 0x0020_0220_0000 != 0 {
            Some(reader.float()?)
        } else {
            None
        };
        let fall_time_ms = reader.word()?;
        let falling = if flags & 0x1000 != 0 {
            Some(ObjectMovementFall {
                vertical_speed: reader.float()?,
                direction_cos: reader.float()?,
                direction_sin: reader.float()?,
                horizontal_speed: reader.float()?,
            })
        } else {
            None
        };
        let spline_elevation = if flags & 0x0400_0000 != 0 {
            Some(reader.float()?)
        } else {
            None
        };
        reader.finish()?;
        Ok(Some(Self {
            kind,
            guid,
            flags,
            position,
            orientation,
            context: ObjectMovementContext {
                timestamp_ms,
                transport,
                pitch_radians,
                fall_time_ms,
                falling,
                spline_elevation,
            },
        }))
    }
}
