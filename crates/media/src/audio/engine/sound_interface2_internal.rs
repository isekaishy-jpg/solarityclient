//! Stock residency and loop policy recovered from `SoundInterface2Internal.cpp`.

use solarity_asset::AssetPath;

use crate::audio::codec::SoundDecodeMode;

const SOUND_ENTRY_LOOP_FLAG: u32 = 0x0000_0200;
const MAXIMUM_CACHEABLE_SIZE_CEILING_BYTES: u32 = 2 * 1024 * 1024;

/// Playback-loop selection passed through the stock sound-kit request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundLoopMode {
    /// Use `SoundEntries.dbc::Flags & 0x200`.
    Entry,
    /// Force an infinite loop for a caller-owned continuous object.
    Loop,
    /// Force a single play even when the base row carries the loop bit.
    Once,
}

impl SoundLoopMode {
    /// Resolves the request override against one exact base-row flag word.
    #[must_use]
    pub const fn is_looping(self, sound_entry_flags: u32) -> bool {
        match self {
            Self::Entry => sound_entry_flags & SOUND_ENTRY_LOOP_FLAG != 0,
            Self::Loop => true,
            Self::Once => false,
        }
    }
}

/// Effective `Sound_MaxCacheableSizeInBytes` admission policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundResidencyPolicy {
    maximum_cacheable_size_bytes: u32,
}

impl SoundResidencyPolicy {
    /// Applies build 12340's hard two-megabyte ceiling to the live CVar word.
    #[must_use]
    pub const fn new(configured_maximum_bytes: u32) -> Self {
        Self {
            maximum_cacheable_size_bytes: if configured_maximum_bytes
                > MAXIMUM_CACHEABLE_SIZE_CEILING_BYTES
            {
                MAXIMUM_CACHEABLE_SIZE_CEILING_BYTES
            } else {
                configured_maximum_bytes
            },
        }
    }

    /// Returns the effective byte threshold after the executable's clamp.
    #[must_use]
    pub const fn maximum_cacheable_size_bytes(self) -> u32 {
        self.maximum_cacheable_size_bytes
    }

    /// Selects FMOD-style sample versus stream admission for one payload.
    ///
    /// Build 12340 always streams MP3 paths. Other supported payloads remain
    /// predecoded only when their archive-reported logical size is at most the
    /// effective cacheable-size threshold.
    #[must_use]
    pub fn decode_mode(self, path: &AssetPath, encoded_size_bytes: usize) -> SoundDecodeMode {
        if path.as_str().ends_with(".MP3")
            || encoded_size_bytes > self.maximum_cacheable_size_bytes as usize
        {
            SoundDecodeMode::Streaming
        } else {
            SoundDecodeMode::Predecoded
        }
    }
}
