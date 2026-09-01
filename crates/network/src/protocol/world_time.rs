//! Authoritative realm clock sent while entering the selected character.

use thiserror::Error;

/// The build-12340 `SMSG_LOGIN_SETTIMESPEED` body.
///
/// The packed calendar and holiday offset are retained even though exterior
/// lighting consumes only the clock-of-day fields. This keeps later calendar
/// and holiday systems on the same authoritative packet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldTimeSpeed {
    packed_time: u32,
    game_time_speed: f32,
    holiday_offset: u32,
}

impl WorldTimeSpeed {
    pub(super) fn decode(payload: &[u8]) -> Result<Self, WorldTimePacketError> {
        if payload.len() != 12 {
            return Err(WorldTimePacketError::new(
                payload.len(),
                "login world time must contain exactly 12 bytes",
            ));
        }
        Self::new(
            read_u32(payload, 0),
            read_f32(payload, 4),
            read_u32(payload, 8),
        )
    }

    /// Creates a validated value from the three wire fields.
    ///
    /// # Errors
    ///
    /// Returns [`WorldTimePacketError`] for an invalid packed calendar, clock,
    /// or rate.
    pub fn new(
        packed_time: u32,
        game_time_speed: f32,
        holiday_offset: u32,
    ) -> Result<Self, WorldTimePacketError> {
        let value = Self {
            packed_time,
            game_time_speed,
            holiday_offset,
        };
        if value.hour() >= 24 || value.minute() >= 60 {
            return Err(WorldTimePacketError::new(
                0,
                "login world time contains an invalid clock",
            ));
        }
        if value.month_index() >= 12 {
            return Err(WorldTimePacketError::new(
                0,
                "login world time contains an invalid month",
            ));
        }
        if value.weekday_index() >= 7 {
            return Err(WorldTimePacketError::new(
                0,
                "login world time contains an invalid weekday",
            ));
        }
        if value.month_day() > days_in_month(value.year(), value.month_index()) {
            return Err(WorldTimePacketError::new(
                0,
                "login world time contains an invalid month day",
            ));
        }
        if !value.game_time_speed.is_finite() || value.game_time_speed < 0.0 {
            return Err(WorldTimePacketError::new(
                4,
                "login world time contains an invalid speed",
            ));
        }
        Ok(value)
    }

    /// Returns the exact packed calendar value carried on the wire.
    #[must_use]
    pub const fn packed_time(self) -> u32 {
        self.packed_time
    }

    /// Returns the realm clock hour.
    #[must_use]
    pub const fn hour(self) -> u8 {
        ((self.packed_time >> 6) & 0x1F) as u8
    }

    /// Returns the realm clock minute.
    #[must_use]
    pub const fn minute(self) -> u8 {
        (self.packed_time & 0x3F) as u8
    }

    /// Returns the full Gregorian year represented by the five-bit wire field.
    #[must_use]
    pub const fn year(self) -> u16 {
        2_000 + ((self.packed_time >> 24) & 0x1F) as u16
    }

    /// Returns the exact zero-based month field carried on the wire.
    #[must_use]
    pub const fn month_index(self) -> u8 {
        ((self.packed_time >> 20) & 0x0F) as u8
    }

    /// Returns the decoded one-based day within the month.
    #[must_use]
    pub const fn month_day(self) -> u8 {
        (((self.packed_time >> 14) & 0x3F) as u8) + 1
    }

    /// Returns the exact zero-based Sunday-through-Saturday wire field.
    #[must_use]
    pub const fn weekday_index(self) -> u8 {
        ((self.packed_time >> 11) & 0x07) as u8
    }

    /// Returns the number of game minutes advanced per real second.
    #[must_use]
    pub const fn game_time_speed(self) -> f32 {
        self.game_time_speed
    }

    /// Returns the post-3.1.2 holiday time adjustment.
    #[must_use]
    pub const fn holiday_offset(self) -> u32 {
        self.holiday_offset
    }
}

const fn days_in_month(year: u16, month_index: u8) -> u8 {
    match month_index {
        0 | 2 | 4 | 6 | 7 | 9 | 11 => 31,
        3 | 5 | 8 | 10 => 30,
        1 if is_leap_year(year) => 29,
        1 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// A malformed `SMSG_LOGIN_SETTIMESPEED` body.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("malformed world-time packet at byte {offset}: {message}")]
pub struct WorldTimePacketError {
    offset: usize,
    message: &'static str,
}

impl WorldTimePacketError {
    const fn new(offset: usize, message: &'static str) -> Self {
        Self { offset, message }
    }

    /// Returns the invalid field offset or packet extent.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns a stable description of the rejected field.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

fn read_u32(payload: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}

fn read_f32(payload: &[u8], offset: usize) -> f32 {
    f32::from_bits(read_u32(payload, offset))
}
