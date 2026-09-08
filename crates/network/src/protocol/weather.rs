//! Build-12340 SMSG_WEATHER (526530, opcode 0x2F4).

use thiserror::Error;

/// An authoritative Weather.dbc selection and precipitation grade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldWeatherUpdate {
    /// Weather.dbc primary key; unknown keys select native clear weather.
    pub weather_id: u32,
    /// Finite server intensity; the receiver clamps this to zero through one.
    pub grade: f32,
    /// Any nonzero wire byte skips the native transition.
    pub instant: bool,
}

/// An invalid weather body cannot enter the environment state machine.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldWeatherPacketError {
    /// The body does not have the exact nine-byte layout.
    #[error("weather packet requires 9 bytes, found {0}")]
    Length(usize),
    /// Non-finite intensity has no meaningful transition.
    #[error("weather grade is not finite")]
    NonFiniteGrade,
}

impl WorldWeatherUpdate {
    pub(super) fn decode(
        opcode: u16,
        body: &[u8],
    ) -> Result<Option<Self>, WorldWeatherPacketError> {
        if opcode != 0x2f4 {
            return Ok(None);
        }
        if body.len() != 9 {
            return Err(WorldWeatherPacketError::Length(body.len()));
        }
        let weather_id = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
        let grade = f32::from_le_bytes([body[4], body[5], body[6], body[7]]);
        if !grade.is_finite() {
            return Err(WorldWeatherPacketError::NonFiniteGrade);
        }
        Ok(Some(Self {
            weather_id,
            grade,
            instant: body[8] != 0,
        }))
    }
}
