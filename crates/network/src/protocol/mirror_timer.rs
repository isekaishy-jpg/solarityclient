//! Build-12340 mirror-timer packets consumed by `519A50`.

use thiserror::Error;

/// A server-owned breath, exhaustion, or feign-death timer notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldMirrorTimerUpdate {
    /// Replaces the timer's complete millisecond anchor.
    Start {
        /// Native slot: exhaustion zero, breath one, feign death two.
        timer: u32,
        /// Signed starting value, without clamping to the range.
        value: i32,
        /// Signed maximum value displayed by FrameXML.
        maximum: i32,
        /// Signed value change per elapsed millisecond.
        scale: i32,
        /// Exact pause byte exposed to Lua as an integer.
        paused: u8,
        /// Optional spell whose localized name replaces the timer label.
        spell_id: u32,
    },
    /// Emits the pause event; the native receiver does not change the anchor.
    Pause {
        /// Native timer identifier, including unknown identifiers.
        timer: u32,
        /// Exact byte carried in the notification.
        paused: u8,
    },
    /// Emits the stop event and clears a valid slot.
    Stop {
        /// Native timer identifier, including unknown identifiers.
        timer: u32,
    },
}

/// A mirror-timer body differs from its exact native wire layout.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed mirror-timer packet: expected {expected} bytes, found {actual}")]
pub struct WorldMirrorTimerPacketError {
    expected: usize,
    actual: usize,
}

impl WorldMirrorTimerUpdate {
    /// Returns the unmodified server timer identifier.
    #[must_use]
    pub const fn timer(self) -> u32 {
        match self {
            Self::Start { timer, .. } | Self::Pause { timer, .. } | Self::Stop { timer } => timer,
        }
    }

    pub(super) fn decode(
        opcode: u16,
        payload: &[u8],
    ) -> Result<Option<Self>, WorldMirrorTimerPacketError> {
        let expected = match opcode {
            0x1d9 => 21,
            0x1da => 5,
            0x1db => 4,
            _ => return Ok(None),
        };
        if payload.len() != expected {
            return Err(WorldMirrorTimerPacketError {
                expected,
                actual: payload.len(),
            });
        }
        let word = |offset| {
            u32::from_le_bytes([
                payload[offset],
                payload[offset + 1],
                payload[offset + 2],
                payload[offset + 3],
            ])
        };
        let timer = word(0);
        Ok(Some(match opcode {
            0x1d9 => Self::Start {
                timer,
                value: word(4) as i32,
                maximum: word(8) as i32,
                scale: word(12) as i32,
                paused: payload[16],
                spell_id: word(17),
            },
            0x1da => Self::Pause {
                timer,
                paused: payload[4],
            },
            _ => Self::Stop { timer },
        }))
    }
}
