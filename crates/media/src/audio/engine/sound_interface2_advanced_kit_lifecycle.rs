//! Stock advanced-kit usage, random-range, and repeat lifecycle policy.

use thiserror::Error;

use super::sound_interface2_advanced_kit_properties::AdvancedSoundProperties;

/// Playback behavior selected by `SoundEntriesAdvanced.dbc::Usage`.
///
/// The meanings are recovered from build 12340's constructor and update
/// routine and corroborated by the shipped rows: state sounds use zero,
/// periodic ambience uses one, and immediate one-shots use two.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AdvancedSoundUsage {
    /// Starts while its daily schedule is active and restarts if interrupted.
    Continuous = 0,
    /// Starts after a randomized repeat countdown while its schedule is active.
    Periodic = 1,
    /// Starts immediately at construction and retires when playback finishes.
    OneShot = 2,
}

impl TryFrom<u32> for AdvancedSoundUsage {
    type Error = AdvancedSoundUsageError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Continuous),
            1 => Ok(Self::Periodic),
            2 => Ok(Self::OneShot),
            value => Err(AdvancedSoundUsageError { value }),
        }
    }
}

/// An advanced row contains no playback behavior known to the stock client.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("unsupported SoundEntriesAdvanced usage value {value}")]
pub struct AdvancedSoundUsageError {
    value: u32,
}

impl AdvancedSoundUsageError {
    /// Returns the uninterpreted DBC word that failed validation.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.value
    }
}

/// One backend operation requested by the stock advanced-kit lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvancedSoundDirective {
    /// Keep the current backend and lifecycle state unchanged.
    None,
    /// Start the advanced sound on its existing user handle.
    Play,
    /// Stop the current variation and immediately start another.
    Restart,
    /// Stop a continuous sound whose authored schedule became inactive.
    Stop,
    /// Destroy the advanced instance after its terminal playback state.
    Retire,
}

/// Per-instance state owned by build 12340's advanced sound service.
///
/// Random words come from the caller so the later runtime integration can use
/// the client-wide Blizzard PRNG and preserve its exact consumption order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdvancedSoundLifecycle {
    usage: AdvancedSoundUsage,
    random_offset_milliseconds: i32,
    repeat_interval_milliseconds: [i32; 2],
    repeat_remaining_milliseconds: i32,
    stopped: bool,
    retire_requested: bool,
}

impl AdvancedSoundLifecycle {
    /// Constructs one live lifecycle and returns its stock initial operation.
    ///
    /// The constructor selects the symmetric schedule offset first and the
    /// repeat interval second. A random word is consumed only when the upper
    /// bound exceeds the lower bound, matching the stock helper.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundUsageError`] rather than assigning fallback
    /// behavior to an unknown usage word.
    pub fn new(
        properties: AdvancedSoundProperties,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> Result<(Self, AdvancedSoundDirective), AdvancedSoundUsageError> {
        let usage = AdvancedSoundUsage::try_from(properties.usage())?;
        let random_offset_range = properties.random_offset_range();
        let random_offset_milliseconds = stock_random_range(
            random_offset_range,
            random_offset_range.wrapping_neg(),
            next_random_word,
        );
        let repeat_interval_milliseconds = properties.repeat_interval_milliseconds();
        let repeat_remaining_milliseconds = stock_random_range(
            repeat_interval_milliseconds[1],
            repeat_interval_milliseconds[0],
            next_random_word,
        );
        let initial_directive = if usage == AdvancedSoundUsage::OneShot {
            AdvancedSoundDirective::Play
        } else {
            AdvancedSoundDirective::None
        };

        Ok((
            Self {
                usage,
                random_offset_milliseconds,
                repeat_interval_milliseconds,
                repeat_remaining_milliseconds,
                stopped: false,
                retire_requested: false,
            },
            initial_directive,
        ))
    }

    /// Advances the exact usage and repeat state by one sound-engine tick.
    ///
    /// `variation_count` is the number of nonempty assets in the resolved
    /// `SoundEntries.dbc` row. Stock only interval-restarts a continuous sound
    /// when this value exceeds one and both authored interval bounds are
    /// positive. Countdown expiration is strictly negative; exactly zero waits
    /// until a later tick.
    #[must_use]
    pub fn update(
        &mut self,
        elapsed_milliseconds: i32,
        schedule_active: bool,
        is_playing: bool,
        variation_count: usize,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> AdvancedSoundDirective {
        if self.retire_requested {
            return AdvancedSoundDirective::Retire;
        }

        if !is_playing {
            if self.usage == AdvancedSoundUsage::OneShot
                || (self.usage == AdvancedSoundUsage::Continuous && self.stopped)
            {
                return AdvancedSoundDirective::Retire;
            }
            if !schedule_active {
                return AdvancedSoundDirective::None;
            }
            return match self.usage {
                AdvancedSoundUsage::Continuous => AdvancedSoundDirective::Play,
                AdvancedSoundUsage::Periodic => {
                    self.repeat_remaining_milliseconds = self
                        .repeat_remaining_milliseconds
                        .wrapping_sub(elapsed_milliseconds);
                    if self.repeat_remaining_milliseconds < 0 {
                        self.reset_repeat_interval(next_random_word);
                        AdvancedSoundDirective::Play
                    } else {
                        AdvancedSoundDirective::None
                    }
                }
                AdvancedSoundUsage::OneShot => AdvancedSoundDirective::Retire,
            };
        }

        if !schedule_active {
            if self.usage == AdvancedSoundUsage::Continuous {
                self.stopped = true;
                return AdvancedSoundDirective::Stop;
            }
            return AdvancedSoundDirective::None;
        }

        let [minimum, maximum] = self.repeat_interval_milliseconds;
        if self.usage == AdvancedSoundUsage::Continuous
            && variation_count > 1
            && minimum > 0
            && maximum > 0
        {
            self.repeat_remaining_milliseconds = self
                .repeat_remaining_milliseconds
                .wrapping_sub(elapsed_milliseconds);
            if self.repeat_remaining_milliseconds < 0 {
                // The stock stop helper leaves this flag set even after the
                // replacement variation begins on the same user handle.
                self.stopped = true;
                self.reset_repeat_interval(next_random_word);
                return AdvancedSoundDirective::Restart;
            }
        }

        AdvancedSoundDirective::None
    }

    /// Requests destruction on the next service update.
    pub const fn request_retire(&mut self) {
        self.retire_requested = true;
    }

    /// Returns the interpreted stock usage mode.
    #[must_use]
    pub const fn usage(self) -> AdvancedSoundUsage {
        self.usage
    }

    /// Returns this instance's constructor-selected schedule offset.
    #[must_use]
    pub const fn random_offset_milliseconds(self) -> i32 {
        self.random_offset_milliseconds
    }

    /// Returns the current repeat countdown for deterministic scheduling.
    #[must_use]
    pub const fn repeat_remaining_milliseconds(self) -> i32 {
        self.repeat_remaining_milliseconds
    }

    /// Selects a fresh repeat interval after a periodic start or variation swap.
    fn reset_repeat_interval(&mut self, next_random_word: &mut impl FnMut() -> u32) {
        self.repeat_remaining_milliseconds = stock_random_range(
            self.repeat_interval_milliseconds[1],
            self.repeat_interval_milliseconds[0],
            next_random_word,
        );
    }
}

/// Maps one Blizzard PRNG word through the client's unsigned multiply-high range.
fn stock_random_range(
    maximum: i32,
    minimum: i32,
    next_random_word: &mut impl FnMut() -> u32,
) -> i32 {
    if maximum <= minimum {
        return maximum;
    }

    let width = maximum.wrapping_sub(minimum) as u32;
    let scaled = (u64::from(next_random_word()) * u64::from(width)) >> u32::BITS;
    minimum.wrapping_add(scaled as i32)
}
