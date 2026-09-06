//! Native movement event packets with a bounded, allocation-free wire image.

use thiserror::Error;

use super::movement::ObjectMovementContext;

/// Ordinary movement events from the native movement and unit dispatchers.
///
/// These packets carry a packed mover GUID and MovementInfo. Teleport and
/// forced-movement acknowledgements have different envelopes and separate owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum WorldMovementKind {
    /// Begin forward translation.
    StartForward = 0xB5,
    /// Begin backward translation.
    StartBackward = 0xB6,
    /// Stop forward/backward translation.
    Stop = 0xB7,
    /// Begin leftward strafe.
    StartStrafeLeft = 0xB8,
    /// Begin rightward strafe.
    StartStrafeRight = 0xB9,
    /// Stop strafing.
    StopStrafe = 0xBA,
    /// Publish a jump launch.
    Jump = 0xBB,
    /// Begin leftward turning.
    StartTurnLeft = 0xBC,
    /// Begin rightward turning.
    StartTurnRight = 0xBD,
    /// Stop turning.
    StopTurn = 0xBE,
    /// Begin upward pitch.
    StartPitchUp = 0xBF,
    /// Begin downward pitch.
    StartPitchDown = 0xC0,
    /// Stop pitching.
    StopPitch = 0xC1,
    /// Clear walking mode; native `0x0098B0E0(1)`.
    SetRunMode = 0xC2,
    /// Set walking mode; native `0x0098B0E0(0)`.
    SetWalkMode = 0xC3,
    /// Publish landing; native unit handler `0x0073D4A0`.
    FallLand = 0xC9,
    /// Enter swimming movement.
    StartSwim = 0xCA,
    /// Leave swimming movement.
    StopSwim = 0xCB,
    /// Publish an explicit facing change.
    SetFacing = 0xDA,
    /// Publish an explicit pitch change; native `0x006E9380`.
    SetPitch = 0xDB,
    /// Publish the periodic movement snapshot.
    Heartbeat = 0xEE,
    /// Retire the previous local mover; native `0x00729010` / `0x0071EF80`.
    NotActiveMover = 0x2D1,
    /// Begin upward swimming/flying translation.
    StartAscend = 0x359,
    /// Stop upward/downward translation.
    StopAscend = 0x35A,
    /// Publish a changed transport attachment; native `0x006EB0B0`.
    ChangeTransport = 0x38D,
    /// Begin downward swimming/flying translation.
    StartDescend = 0x3A7,
}

/// Conditional section whose presence must agree with the supplied wire flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldMovementField {
    /// ON_TRANSPORT attachment section.
    Transport,
    /// Second clock inside an interpolated attachment.
    TransportInterpolation,
    /// Swimming/flying/always-pitching angle.
    Pitch,
    /// FALLING launch parameters.
    Falling,
    /// SPLINE_ELEVATION value.
    SplineElevation,
}

/// An inconsistent application snapshot rejected before touching header crypto.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldMovementEncodeError {
    /// The caller supplied bits beyond the native 32+16-bit flag envelope.
    #[error("movement flags exceed the 48-bit wire field: {flags:#018x}")]
    FlagWidth {
        /// Exact rejected flag word.
        flags: u64,
    },
    /// The caller's optional data disagrees with its own field-presence flags.
    #[error("movement field {field:?} presence disagrees with the wire flags")]
    FieldPresence {
        /// Conditional section requiring correction by the movement owner.
        field: WorldMovementField,
    },
}

// Native maximum: packed mover GUID 9 + base MovementInfo 26 + transport 34
// + pitch 4 + fall time 4 + launch 16 + spline elevation 4 = 97 bytes.
const MAX_MOVEMENT_BODY_BYTES: usize = 97;

/// One validated, immutable movement packet ready for the sole session writer.
///
/// `0x0071EF80` writes the mover GUID; `0x004F4ED0` writes MovementInfo.
/// The image owns the event-time snapshot, so queue delay cannot change its
/// position, timestamp, attachment, or launch values. Construction allocates
/// nothing and rejects shape mismatches before a cipher can be advanced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldMovementMessage {
    kind: WorldMovementKind,
    body: [u8; MAX_MOVEMENT_BODY_BYTES],
    length: u8,
}

impl WorldMovementMessage {
    /// Encodes a snapshot already resolved by the movement simulation owner.
    ///
    /// Flags are wire flags after native movement/control policy. This codec
    /// does not change movement modes, normalize facing, or invent missing
    /// trajectory fields. Float bit patterns and wrapping integer clocks are
    /// preserved exactly, as in the native scalar writer.
    ///
    /// # Errors
    ///
    /// Returns [`WorldMovementEncodeError`] for flags wider than 48 bits or
    /// conditional fields whose presence disagrees with those flags.
    pub fn new(
        kind: WorldMovementKind,
        mover_guid: u64,
        flags: u64,
        position: [f32; 3],
        orientation: f32,
        context: ObjectMovementContext,
    ) -> Result<Self, WorldMovementEncodeError> {
        if flags >> 48 != 0 {
            return Err(WorldMovementEncodeError::FlagWidth { flags });
        }
        validate_presence(
            flags & 0x200 != 0,
            context.transport.is_some(),
            WorldMovementField::Transport,
        )?;
        if let Some(transport) = context.transport {
            validate_presence(
                flags & 0x0400_0000_0000 != 0,
                transport.interpolated_time_ms.is_some(),
                WorldMovementField::TransportInterpolation,
            )?;
        }
        validate_presence(
            flags & 0x0020_0220_0000 != 0,
            context.pitch_radians.is_some(),
            WorldMovementField::Pitch,
        )?;
        validate_presence(
            flags & 0x1000 != 0,
            context.falling.is_some(),
            WorldMovementField::Falling,
        )?;
        validate_presence(
            flags & 0x0400_0000 != 0,
            context.spline_elevation.is_some(),
            WorldMovementField::SplineElevation,
        )?;
        let mut message = Self {
            kind,
            body: [0; MAX_MOVEMENT_BODY_BYTES],
            length: 0,
        };
        message.push_guid(mover_guid);
        message.push(&(flags as u32).to_le_bytes());
        message.push(&((flags >> 32) as u16).to_le_bytes());
        message.push(&context.timestamp_ms.to_le_bytes());
        for value in position.into_iter().chain([orientation]) {
            message.push(&value.to_le_bytes());
        }
        if let Some(transport) = context.transport {
            message.push_guid(transport.guid);
            for value in transport
                .position
                .into_iter()
                .chain([transport.orientation])
            {
                message.push(&value.to_le_bytes());
            }
            message.push(&transport.time_ms.to_le_bytes());
            message.push(&[transport.seat as u8]);
            if let Some(time) = transport.interpolated_time_ms {
                message.push(&time.to_le_bytes());
            }
        }
        if let Some(pitch) = context.pitch_radians {
            message.push(&pitch.to_le_bytes());
        }
        // Native 0x004F4ED0 calls the integer writer for fall time. The pinned
        // third-party MovementInfo calls this field f32; do not numerically
        // convert the elapsed milliseconds through that incompatible type.
        message.push(&context.fall_time_ms.to_le_bytes());
        if let Some(falling) = context.falling {
            for value in [
                falling.vertical_speed,
                falling.direction_cos,
                falling.direction_sin,
                falling.horizontal_speed,
            ] {
                message.push(&value.to_le_bytes());
            }
        }
        if let Some(elevation) = context.spline_elevation {
            message.push(&elevation.to_le_bytes());
        }
        Ok(message)
    }

    /// Returns the event represented by this frozen wire image.
    #[must_use]
    pub const fn kind(&self) -> WorldMovementKind {
        self.kind
    }

    pub(crate) fn body(&self) -> &[u8] {
        &self.body[..usize::from(self.length)]
    }

    /// Appends one fixed-width field; the closed native layout bounds all calls.
    fn push(&mut self, bytes: &[u8]) {
        let start = usize::from(self.length);
        let end = start + bytes.len();
        self.body[start..end].copy_from_slice(bytes);
        self.length = end as u8;
    }

    /// Emits the native presence mask and nonzero GUID octets in byte order.
    fn push_guid(&mut self, guid: u64) {
        let bytes = guid.to_le_bytes();
        let mask = bytes.iter().enumerate().fold(0_u8, |mask, (bit, value)| {
            mask | (u8::from(*value != 0) << bit)
        });
        self.push(&[mask]);
        for byte in bytes {
            if byte != 0 {
                self.push(&[byte]);
            }
        }
    }
}

/// Requires the application to supply precisely the conditional native layout.
fn validate_presence(
    expected: bool,
    supplied: bool,
    field: WorldMovementField,
) -> Result<(), WorldMovementEncodeError> {
    if expected != supplied {
        return Err(WorldMovementEncodeError::FieldPresence { field });
    }
    Ok(())
}
