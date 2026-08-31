//! Stock residency and loop policy recovered from `SoundInterface2Internal.cpp`.

use solarity_asset::AssetPath;

use crate::audio::codec::SoundDecodeMode;

const SOUND_ENTRY_LOOP_FLAG: u32 = 0x0000_0200;
const SOUND_ENTRY_EXCLUSIVE_FLAG: u32 = 0x0000_0020;
const MAXIMUM_CACHEABLE_SIZE_CEILING_BYTES: u32 = 2 * 1024 * 1024;
const MINIMUM_SAMPLE_CACHE_SIZE_BYTES: u32 = 4 * 1024 * 1024;
const LARGE_SAMPLE_CACHE_BOUNDARY_BYTES: u32 = 100 * 1024 * 1024;
const LARGE_SAMPLE_CACHE_SIZE_BYTES: u32 = 128 * 1024 * 1024;

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

/// Same-entry concurrency selection carried by the stock play options.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundConcurrencyMode {
    /// Use `SoundEntries.dbc::Flags & 0x20`.
    Entry,
    /// Reject playback while the same entry is already active.
    Exclusive,
    /// Permit another instance even when the base row is exclusive.
    Concurrent,
}

impl SoundConcurrencyMode {
    /// Resolves the request override against one exact base-row flag word.
    #[must_use]
    pub const fn is_exclusive(self, sound_entry_flags: u32) -> bool {
        match self {
            Self::Entry => sound_entry_flags & SOUND_ENTRY_EXCLUSIVE_FLAG != 0,
            Self::Exclusive => true,
            Self::Concurrent => false,
        }
    }
}

/// Effective `Sound_MaxCacheableSizeInBytes` admission policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundResidencyPolicy {
    maximum_cacheable_size_bytes: u32,
    maximum_sample_cache_size_bytes: u32,
}

impl SoundResidencyPolicy {
    /// Applies build 12340's hard two-megabyte ceiling to the live CVar word.
    #[must_use]
    pub const fn new(
        configured_maximum_cacheable_bytes: u32,
        configured_sample_cache_bytes: u32,
    ) -> Self {
        Self {
            maximum_cacheable_size_bytes: if configured_maximum_cacheable_bytes
                > MAXIMUM_CACHEABLE_SIZE_CEILING_BYTES
            {
                MAXIMUM_CACHEABLE_SIZE_CEILING_BYTES
            } else {
                configured_maximum_cacheable_bytes
            },
            maximum_sample_cache_size_bytes: if configured_sample_cache_bytes
                < MINIMUM_SAMPLE_CACHE_SIZE_BYTES
            {
                MINIMUM_SAMPLE_CACHE_SIZE_BYTES
            } else if configured_sample_cache_bytes > LARGE_SAMPLE_CACHE_BOUNDARY_BYTES {
                LARGE_SAMPLE_CACHE_SIZE_BYTES
            } else {
                configured_sample_cache_bytes
            },
        }
    }

    /// Returns the effective byte threshold after the executable's clamp.
    #[must_use]
    pub const fn maximum_cacheable_size_bytes(self) -> u32 {
        self.maximum_cacheable_size_bytes
    }

    /// Returns the effective total predecoded-sample budget.
    #[must_use]
    pub const fn maximum_sample_cache_size_bytes(self) -> u32 {
        self.maximum_sample_cache_size_bytes
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
