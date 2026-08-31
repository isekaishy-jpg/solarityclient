//! Dependency-neutral identity and diagnostics for decoded audio resources.

use solarity_asset::AssetPath;

/// Caller-selected residency strategy for one encoded sound.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SoundDecodeMode {
    /// Retain encoded bytes and decode them incrementally during mixing.
    Streaming,
    /// Decode the complete source to PCM during admission.
    Predecoded,
}

/// Stable decoder-local reference to one SDL audio resource.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DecodedSoundHandle {
    pub(super) decoder_id: u64,
    pub(super) slot: u32,
}

/// Observable format and residency data without exposing SDL types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedSoundInfo {
    path: AssetPath,
    mode: SoundDecodeMode,
    sample_rate_hz: u32,
    channel_count: u8,
    duration_frames: Option<u64>,
}

impl DecodedSoundInfo {
    /// Captures the validated SDL resource description after admission.
    pub(super) const fn new(
        path: AssetPath,
        mode: SoundDecodeMode,
        sample_rate_hz: u32,
        channel_count: u8,
        duration_frames: Option<u64>,
    ) -> Self {
        Self {
            path,
            mode,
            sample_rate_hz,
            channel_count,
            duration_frames,
        }
    }

    /// Returns the normalized encoded-source identity.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the explicit admission strategy.
    #[must_use]
    pub const fn mode(&self) -> SoundDecodeMode {
        self.mode
    }

    /// Returns decoded frames per second.
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns the decoded interleaved channel count.
    #[must_use]
    pub const fn channel_count(&self) -> u8 {
        self.channel_count
    }

    /// Returns the decoded frame length when the codec reports one.
    #[must_use]
    pub const fn duration_frames(&self) -> Option<u64> {
        self.duration_frames
    }
}
