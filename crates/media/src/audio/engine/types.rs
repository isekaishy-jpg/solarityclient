//! Dependency-neutral sound-engine policy and request vocabulary.

use crate::audio::backend::SoundVoiceHandle;
use crate::audio::codec::SoundDecodeMode;
use crate::audio::selection::SoundVariationMode;

use super::status::{SoundCategoryError, SoundGainError};

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

impl SoundCategory {
    /// Interprets `SoundEntriesAdvanced.dbc::VolumeSliderCategory` exactly.
    ///
    /// # Errors
    ///
    /// Returns [`SoundCategoryError`] for words outside the executable's
    /// eighteen-entry channel table.
    pub fn from_volume_slider_category(value: u32) -> Result<Self, SoundCategoryError> {
        match value {
            0 | 7..=17 => Ok(Self::Sfx),
            1 | 5 => Ok(Self::Music),
            2 => Ok(Self::Ambience),
            3 => Ok(Self::Cinematic),
            4 => Ok(Self::ScriptSound),
            6 => Ok(Self::RacialCinematic),
            value => Err(SoundCategoryError { value }),
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
    ) -> Self {
        Self {
            enabled,
            master_gain,
            sfx,
            music,
            ambience,
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
    category: SoundCategory,
    variation_mode: SoundVariationMode,
    decode_mode: SoundDecodeMode,
    looping: bool,
}

impl SoundPlayRequest {
    /// Captures a request without inventing category, selection, or residency policy.
    #[must_use]
    pub const fn new(
        entry_id: u32,
        category: SoundCategory,
        variation_mode: SoundVariationMode,
        decode_mode: SoundDecodeMode,
        looping: bool,
    ) -> Self {
        Self {
            entry_id,
            category,
            variation_mode,
            decode_mode,
            looping,
        }
    }

    /// Returns the exact `SoundEntries.dbc` identifier.
    #[must_use]
    pub const fn entry_id(self) -> u32 {
        self.entry_id
    }

    /// Returns the caller-owned volume category.
    #[must_use]
    pub const fn category(self) -> SoundCategory {
        self.category
    }

    /// Returns the exact stock selection mode for this call site.
    #[must_use]
    pub const fn variation_mode(self) -> SoundVariationMode {
        self.variation_mode
    }

    /// Returns the explicit decoder residency strategy.
    #[must_use]
    pub const fn decode_mode(self) -> SoundDecodeMode {
        self.decode_mode
    }

    /// Reports whether playback repeats indefinitely.
    #[must_use]
    pub const fn looping(self) -> bool {
        self.looping
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
