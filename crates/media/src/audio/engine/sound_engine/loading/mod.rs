//! Ordered selection, observable request lifetimes and nonblocking sound admission.

mod commands;
mod request;

pub use request::{SoundLoadHandle, SoundLoadRequest};

use super::{SoundChannel, positioning::PositionedSoundSource};
use crate::audio::{
    backend::SoundVoicePriority,
    codec::SoundDecodeTicket,
    engine::{AdvancedSoundInstanceId, AdvancedSoundListener, SoundResidencyPolicy},
};

/// Admission policy retained while bytes are read, before an SDL track exists.
pub(super) struct PendingVoice {
    load: SoundLoadRequest,
    // Unique reservation ownership invalidates every request clone on all exits.
    _lifetime: request::PendingLoadLifetime,
    pub(super) entry_id: Option<u32>,
    pub(super) channel: SoundChannel,
    source_gain: f32,
    frequency_ratio: f32,
    spatial_source: Option<(PositionedSoundSource, AdvancedSoundListener)>,
    fade: super::SoundFade,
    looping: bool,
    priority: SoundVoicePriority,
    duck_source: Option<AdvancedSoundInstanceId>,
    residency: SoundResidencyPolicy,
    pub(super) decode: Option<SoundDecodeTicket>,
}
