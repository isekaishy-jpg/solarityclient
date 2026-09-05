//! Generic GameObject states and sequence seeks from build-12340 behavior code.

/// Internal states shared by presentation and the door collision override.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GameObjectAnimationState {
    /// An explicitly requested spawn animation.
    Spawn = 0,
    /// Closed and solid for the door behavior.
    Closed = 1,
    /// Playing Open toward the opened state.
    Opening = 2,
    /// Stable opened pose.
    Opened = 3,
    /// Playing Close toward the closed state.
    Closing = 4,
    /// Playing Destroy toward the destroyed state.
    Destroying = 5,
    /// Stable destroyed pose.
    Destroyed = 6,
    /// Playing Rebuild toward the closed state.
    Rebuilding = 7,
}

impl GameObjectAnimationState {
    /// Resolves initialization or full refresh through native `0x0070D600`.
    #[must_use]
    pub const fn initial(state: u8, progress: Option<u16>) -> Option<Self> {
        match (state, progress.is_some()) {
            (0, false) => Some(Self::Opened),
            (0, true) => Some(Self::Opening),
            (1, false) => Some(Self::Closed),
            (1, true) => Some(Self::Closing),
            (2, false) => Some(Self::Destroyed),
            (2, true) => Some(Self::Destroying),
            _ => None,
        }
    }

    /// Resolves a state notification through native `0x0070D690`.
    /// Previous state matters even without a supplied sequence fraction.
    #[must_use]
    pub const fn changed(previous: u8, next: u8, progress: Option<u16>) -> Option<Self> {
        match next {
            0 => Some(if previous != 1 && progress.is_none() {
                Self::Opened
            } else {
                Self::Opening
            }),
            1 => Some(if progress.is_some() || previous == 0 {
                Self::Closing
            } else if previous == 2 {
                Self::Rebuilding
            } else {
                Self::Closed
            }),
            2 => Some(if previous != 1 && progress.is_none() {
                Self::Destroyed
            } else {
                Self::Destroying
            }),
            _ => None,
        }
    }

    /// Resolves `0x0070D7E0`; stable states request their sequence again.
    #[must_use]
    pub const fn completed(self, replicated: u8, progress: Option<u16>) -> Option<Self> {
        match self {
            Self::Spawn => Self::changed(replicated, replicated, progress),
            Self::Opening => Some(Self::Opened),
            Self::Closing | Self::Rebuilding => Some(Self::Closed),
            Self::Destroying => Some(Self::Destroyed),
            stable => Some(stable),
        }
    }

    /// Returns the native animation request from table `0x00ADA938`.
    #[must_use]
    pub const fn animation_id(self) -> u16 {
        match self {
            Self::Spawn => 145,
            Self::Closed => 147,
            Self::Opening => 148,
            Self::Opened => 149,
            Self::Closing => 146,
            Self::Destroying => 150,
            Self::Destroyed => 151,
            Self::Rebuilding => 152,
        }
    }

    /// Returns whether sequence setup consumes supplied progress and clock flags.
    #[must_use]
    pub const fn is_transition(self) -> bool {
        matches!(
            self,
            Self::Opening | Self::Closing | Self::Destroying | Self::Rebuilding
        )
    }

    /// Tests the native reversal pairs from `0x0070D510`.
    #[must_use]
    pub const fn reverses(self, previous: Self) -> bool {
        matches!(
            (previous, self),
            (Self::Opening, Self::Closing)
                | (Self::Closing, Self::Opening)
                | (Self::Destroying, Self::Rebuilding)
                | (Self::Rebuilding, Self::Destroying)
        )
    }

    /// Returns the door-specific predicate from `0x0070D8D0` / `0x00712550`.
    #[must_use]
    pub const fn door_collision_eligible(self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// Converts a supplied fraction using native x87 arithmetic and its f32 spill.
/// The final integer conversion rounds ties to even instead of truncating.
#[must_use]
pub fn game_object_sequence_offset(duration_ms: u32, progress: u16) -> i32 {
    let fraction = f64::from(progress) * f64::from(f32::from_bits(0x3780_0080));
    native_rounded_i32((f64::from(duration_ms) * fraction) as f32)
}

/// Converts a live primary timer to the reversed ushort seek (`0x0070D510`).
/// A nonpositive span or an already expired timer supplies no reversed seek.
#[must_use]
pub fn game_object_reversed_progress(scene_ms: u32, start_ms: u32, end_ms: u32) -> Option<u16> {
    let span = end_ms.wrapping_sub(start_ms) as i32;
    if span <= 0 {
        return None;
    }
    let elapsed = scene_ms.wrapping_sub(start_ms);
    let fraction = f64::from(elapsed) / f64::from(span);
    if fraction > 1.0 {
        return None;
    }
    let reversed = (1.0 - fraction) as f32;
    Some(native_rounded_i32(reversed * 65_535.0) as u16)
}

fn native_rounded_i32(value: f32) -> i32 {
    let rounded = f64::from(value).round_ties_even();
    if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        i32::MIN
    } else {
        rounded as i32
    }
}
