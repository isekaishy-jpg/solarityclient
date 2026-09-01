//! Validated realm-clock values projected from the active world session.

use thiserror::Error;

/// One exact hour and minute returned by the native `GetGameTime` query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiRealmTime {
    hour: u8,
    minute: u8,
}

impl UiRealmTime {
    /// Creates one time within stock's cyclic 24-hour realm day.
    ///
    /// # Errors
    ///
    /// Returns [`UiRealmTimeError`] when the hour or minute is outside its
    /// native range.
    pub const fn new(hour: u8, minute: u8) -> Result<Self, UiRealmTimeError> {
        if hour >= 24 {
            return Err(UiRealmTimeError::Hour { hour });
        }
        if minute >= 60 {
            return Err(UiRealmTimeError::Minute { minute });
        }
        Ok(Self { hour, minute })
    }

    /// Returns the zero-based hour in the inclusive range zero through 23.
    #[must_use]
    pub const fn hour(self) -> u8 {
        self.hour
    }

    /// Returns the zero-based minute in the inclusive range zero through 59.
    #[must_use]
    pub const fn minute(self) -> u8 {
        self.minute
    }
}

/// Invalid native value supplied to the FrameXML realm-clock boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UiRealmTimeError {
    /// Hours use a cyclic 24-hour domain.
    #[error("realm hour {hour} is outside 0 through 23")]
    Hour {
        /// Rejected hour supplied by the native world projection.
        hour: u8,
    },
    /// Minutes use a cyclic 60-minute domain.
    #[error("realm minute {minute} is outside 0 through 59")]
    Minute {
        /// Rejected minute supplied by the native world projection.
        minute: u8,
    },
}
