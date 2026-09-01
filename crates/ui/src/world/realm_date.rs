//! Validated realm-calendar values projected from the active world session.

use thiserror::Error;

/// One exact Gregorian date returned by the native `CalendarGetDate` query.
///
/// Weekdays and months are stored in Lua's one-based domain because stock
/// FrameXML uses both values directly as table indexes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiRealmDate {
    weekday: u8,
    month: u8,
    month_day: u8,
    year: u16,
}

impl UiRealmDate {
    /// Creates one complete realm-calendar projection.
    ///
    /// # Errors
    ///
    /// Returns [`UiRealmDateError`] when any native value is outside its
    /// Gregorian or Lua-facing range.
    pub const fn new(
        weekday: u8,
        month: u8,
        month_day: u8,
        year: u16,
    ) -> Result<Self, UiRealmDateError> {
        if weekday == 0 || weekday > 7 {
            return Err(UiRealmDateError::Weekday { weekday });
        }
        if month == 0 || month > 12 {
            return Err(UiRealmDateError::Month { month });
        }
        if year == 0 {
            return Err(UiRealmDateError::Year { year });
        }
        let maximum = days_in_month(year, month);
        if month_day == 0 || month_day > maximum {
            return Err(UiRealmDateError::MonthDay {
                month_day,
                month,
                year,
            });
        }
        Ok(Self {
            weekday,
            month,
            month_day,
            year,
        })
    }

    /// Returns Sunday as one through Saturday as seven for Lua table indexing.
    #[must_use]
    pub const fn weekday(self) -> u8 {
        self.weekday
    }

    /// Returns January as one through December as 12.
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// Returns the one-based day within the current month.
    #[must_use]
    pub const fn month_day(self) -> u8 {
        self.month_day
    }

    /// Returns the complete Gregorian year.
    #[must_use]
    pub const fn year(self) -> u16 {
        self.year
    }
}

/// Invalid native value supplied to the FrameXML realm-calendar boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UiRealmDateError {
    /// Stock FrameXML indexes a seven-element Sunday-first name table.
    #[error("realm weekday {weekday} is outside 1 through 7")]
    Weekday {
        /// Rejected weekday supplied by the native world projection.
        weekday: u8,
    },
    /// Stock FrameXML indexes a 12-element January-first name table.
    #[error("realm month {month} is outside 1 through 12")]
    Month {
        /// Rejected month supplied by the native world projection.
        month: u8,
    },
    /// Gregorian years have no year zero in this UI boundary.
    #[error("realm year {year} is outside the Gregorian UI domain")]
    Year {
        /// Rejected year supplied by the native world projection.
        year: u16,
    },
    /// Month-day validation accounts for month length and leap years.
    #[error("realm day {month_day} is invalid for month {month} of year {year}")]
    MonthDay {
        /// Rejected one-based day within the month.
        month_day: u8,
        /// One-based month against which the day was checked.
        month: u8,
        /// Gregorian year against which February was checked.
        year: u16,
    },
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}
