//! Dependency-neutral sound-engine policy and request vocabulary.

use crate::audio::backend::{SoundVoiceHandle, SoundVoicePriority};
use crate::audio::selection::SoundVariationMode;

use super::status::{SoundChannelError, SoundGainError};
use super::{AdvancedSoundInstanceId, SoundConcurrencyMode, SoundLoopMode, SoundResidencyPolicy};

/// Real FMOD software-channel count selected during sound initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundSoftwareChannelCount(u16);

impl SoundSoftwareChannelCount {
    /// Applies build 12340's signed `12..=128` initialization clamp.
    #[must_use]
    pub const fn new(configured: i32) -> Self {
        if configured < 12 {
            Self(12)
        } else if configured > 128 {
            Self(128)
        } else {
            Self(configured as u16)
        }
    }

    /// Returns the clamped real software-mix count.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Stock volume-control category selected by the calling subsystem.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundCategory {
    /// Interface, spell, unit, item, and other effects.
    Sfx,
    /// Foreground and zone music.
    Music,
    /// Glue, zone, and positioned environment ambience.
    Ambience,
    /// Cinematic playback controlled by master policy only.
    Cinematic,
    /// Script-sound playback controlled by master policy only.
    ScriptSound,
    /// Racial cinematic playback controlled by master policy only.
    RacialCinematic,
}

/// Exact index into build 12340's eighteen-entry sound-channel table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SoundChannel(u8);

impl SoundChannel {
    /// General sound-effects channel.
    pub const SFX: Self = Self(0);
    /// Foreground music channel.
    pub const MUSIC: Self = Self(1);
    /// World ambience channel.
    pub const AMBIENCE: Self = Self(2);

    /// Validates a caller or `VolumeSliderCategory` channel word.
    ///
    /// # Errors
    ///
    /// Returns [`SoundChannelError`] outside the executable's channel table.
    pub const fn new(value: u32) -> Result<Self, SoundChannelError> {
        if value <= 17 {
            Ok(Self(value as u8))
        } else {
            Err(SoundChannelError { value })
        }
    }

    /// Returns the exact numeric channel index.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    /// Returns the CVar volume group attached to this channel.
    #[must_use]
    pub const fn category(self) -> SoundCategory {
        match self.0 {
            0 | 7..=17 => SoundCategory::Sfx,
            1 | 5 => SoundCategory::Music,
            2 => SoundCategory::Ambience,
            3 => SoundCategory::Cinematic,
            4 => SoundCategory::ScriptSound,
            6 => SoundCategory::RacialCinematic,
            _ => unreachable!(),
        }
    }

    /// Returns the channel-layer simultaneous-instance cap when finite.
    #[must_use]
    pub const fn maximum_active_voices(self) -> Option<usize> {
        match self.0 {
            0..=5 => None,
            6 | 7 | 9 | 12 | 15 => Some(1),
            8 | 10 | 11 | 16 => Some(2),
            13 => Some(6),
            14 | 17 => Some(4),
            _ => unreachable!(),
        }
    }
}

/// Validated stock CVar gain in the inclusive zero-to-one range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundGain(f32);

impl SoundGain {
    /// Validates a master or category gain before it reaches active voices.
    ///
    /// # Errors
    ///
    /// Returns [`SoundGainError`] for a non-finite value or one outside the
    /// stock CVar range.
    pub fn new(value: f32) -> Result<Self, SoundGainError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(SoundGainError { value })
        }
    }

    /// Returns the validated scalar.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.0
    }
}

/// Enablement and gain for one stock sound category.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundCategorySettings {
    enabled: bool,
    gain: SoundGain,
}

impl SoundCategorySettings {
    /// Captures one already validated category policy.
    #[must_use]
    pub const fn new(enabled: bool, gain: SoundGain) -> Self {
        Self { enabled, gain }
    }

    /// Reports whether new and active voices in this category are audible.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Returns the category gain applied after master gain.
    #[must_use]
    pub const fn gain(self) -> SoundGain {
        self.gain
    }
}

/// Complete live policy sourced from stock sound CVars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundEngineSettings {
    enabled: bool,
    master_gain: SoundGain,
    sfx: SoundCategorySettings,
    music: SoundCategorySettings,
    ambience: SoundCategorySettings,
    residency: SoundResidencyPolicy,
}

impl SoundEngineSettings {
    /// Captures an explicit, fully validated CVar snapshot.
    #[must_use]
    pub const fn new(
        enabled: bool,
        master_gain: SoundGain,
        sfx: SoundCategorySettings,
        music: SoundCategorySettings,
        ambience: SoundCategorySettings,
        residency: SoundResidencyPolicy,
    ) -> Self {
        Self {
            enabled,
            master_gain,
            sfx,
            music,
            ambience,
            residency,
        }
    }

    /// Reports the process-wide `Sound_EnableAllSound` state.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Returns the process-wide master gain.
    #[must_use]
    pub const fn master_gain(self) -> SoundGain {
        self.master_gain
    }

    /// Returns policy for one calling subsystem's category.
    #[must_use]
    pub const fn category(self, category: SoundCategory) -> SoundCategorySettings {
        match category {
            SoundCategory::Sfx => self.sfx,
            SoundCategory::Music => self.music,
            SoundCategory::Ambience => self.ambience,
            SoundCategory::Cinematic
            | SoundCategory::ScriptSound
            | SoundCategory::RacialCinematic => SoundCategorySettings::new(true, SoundGain(1.0)),
        }
    }

    /// Returns the effective stock encoded-payload residency policy.
    #[must_use]
    pub const fn residency(self) -> SoundResidencyPolicy {
        self.residency
    }

    /// Computes audible gain before the authored per-entry multiplier.
    pub(super) fn category_gain(self, category: SoundCategory) -> Option<f32> {
        let settings = self.category(category);
        (self.enabled && settings.enabled())
            .then_some(self.master_gain.value() * settings.gain().value())
    }
}

/// One fully explicit request whose playback call advances stock randomness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundPlayRequest {
    entry_id: u32,
    channel: SoundChannel,
    variation_mode: SoundVariationMode,
    loop_mode: SoundLoopMode,
    concurrency_mode: SoundConcurrencyMode,
    priority: SoundVoicePriority,
    advanced_source: Option<AdvancedSoundInstanceId>,
}

impl SoundPlayRequest {
    /// Captures a request without inventing category or selection policy.
    #[must_use]
    pub const fn new(
        entry_id: u32,
        channel: SoundChannel,
        variation_mode: SoundVariationMode,
        loop_mode: SoundLoopMode,
        concurrency_mode: SoundConcurrencyMode,
    ) -> Self {
        Self {
            entry_id,
            channel,
            variation_mode,
            loop_mode,
            concurrency_mode,
            priority: SoundVoicePriority::DEFAULT,
            advanced_source: None,
        }
    }

    /// Captures a service-owned advanced request with duck self-exclusion.
    pub(super) const fn advanced(
        entry_id: u32,
        channel: SoundChannel,
        variation_mode: SoundVariationMode,
        loop_mode: SoundLoopMode,
        concurrency_mode: SoundConcurrencyMode,
        advanced_source: AdvancedSoundInstanceId,
    ) -> Self {
        Self {
            entry_id,
            channel,
            variation_mode,
            loop_mode,
            concurrency_mode,
            priority: SoundVoicePriority::DEFAULT,
            advanced_source: Some(advanced_source),
        }
    }

    /// Replaces the stock default play-option priority word.
    #[must_use]
    pub const fn with_priority(mut self, priority: SoundVoicePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Returns the exact `SoundEntries.dbc` identifier.
    #[must_use]
    pub const fn entry_id(self) -> u32 {
        self.entry_id
    }

    /// Returns the caller-owned exact sound channel.
    #[must_use]
    pub const fn channel(self) -> SoundChannel {
        self.channel
    }

    /// Returns the exact stock selection mode for this call site.
    #[must_use]
    pub const fn variation_mode(self) -> SoundVariationMode {
        self.variation_mode
    }

    /// Returns the stock row/override loop selection for this call site.
    #[must_use]
    pub const fn loop_mode(self) -> SoundLoopMode {
        self.loop_mode
    }

    /// Returns the base-row/override same-entry concurrency selection.
    #[must_use]
    pub const fn concurrency_mode(self) -> SoundConcurrencyMode {
        self.concurrency_mode
    }

    /// Returns the exact signed priority word from the play options.
    #[must_use]
    pub const fn priority(self) -> SoundVoicePriority {
        self.priority
    }

    /// Returns the advanced instance excluded from its own duck influence.
    pub(super) const fn advanced_source(self) -> Option<AdvancedSoundInstanceId> {
        self.advanced_source
    }
}

/// Result of applying enablement policy to a valid request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundPlayback {
    /// A backend voice owns the selected sound.
    Started(SoundVoiceHandle),
    /// Global or category CVar policy disabled playback before asset admission.
    Suppressed,
}
