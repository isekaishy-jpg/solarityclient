//! External stock-compatibility tests for fixed-capacity SDL output voices.

use std::error::Error;
use std::num::NonZeroU16;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    SoundBackend, SoundBackendError, SoundCache, SoundDecodeMode, SoundDecoder, SoundOutput,
    SoundOutputTarget, SoundSpatialPosition, SoundVoicePriority, SoundVoiceState,
};

use crate::support::{Fixture, FixtureFile, pcm_wav, sdl_test_lock};

/// Priority chooses the one real voice while both virtual timelines advance.
#[test]
fn memory_output_preserves_explicit_voice_capacity() -> Result<(), Box<dyn Error>> {
    let samples = [0, 12_000, 0, -12_000].repeat(2_000);
    let wav = pcm_wav(8_000, &samples)?;
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Sound\\Test\\Loop.wav",
        bytes: &wav,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let path = AssetPath::new("Sound/Test/Loop.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;

    let _sdl_test = sdl_test_lock();
    let mut decoder = SoundDecoder::new()?;
    let sound = decoder.load(&encoded, SoundDecodeMode::Predecoded)?;
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    assert_eq!(output.info().target(), SoundOutputTarget::Memory);
    assert_eq!(output.info().sample_rate_hz(), 44_100);
    assert_eq!(output.info().channel_count(), 2);
    let capacity = NonZeroU16::new(1).ok_or("voice capacity is zero")?;
    let virtual_capacity = NonZeroU16::new(2).ok_or("voice capacity is zero")?;
    let mut backend = SoundBackend::new(&output, capacity, virtual_capacity)?;
    assert_eq!(backend.software_channel_count(), 1);
    assert_eq!(backend.voice_capacity(), 2);

    let voice = backend
        .play(&decoder, sound, 0.5, true, SoundVoicePriority::DEFAULT)?
        .voice();
    assert_eq!(backend.state(voice)?, SoundVoiceState::Playing);
    let more_important = backend
        .play(&decoder, sound, 0.5, true, SoundVoicePriority::new(100))?
        .voice();
    assert_eq!(backend.state(more_important)?, SoundVoiceState::Playing);
    assert!(backend.is_virtual(voice)?);
    assert!(!backend.is_virtual(more_important)?);
    backend.pause(voice)?;
    assert_eq!(backend.state(voice)?, SoundVoiceState::Paused);
    backend.resume(voice)?;
    backend.set_gain(voice, 1.5)?;

    let mut mixed = [0_u8; 4_096];
    let mixed_byte_count = backend.generate(&mut mixed)?;
    assert!(mixed_byte_count > 0);
    assert!(mixed_byte_count <= mixed.len());
    assert!(mixed.iter().any(|byte| *byte != 0));
    backend.stop(voice)?;
    assert_eq!(backend.state(voice)?, SoundVoiceState::Stopped);
    backend.stop(more_important)?;
    Ok(())
}

/// SDL spatial positioning receives explicit listener-relative coordinates.
#[test]
fn backend_positions_voice_in_the_listener_frame() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, &vec![12_000; 8_000])?;
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Sound\\Test\\Position.wav",
        bytes: &wav,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let encoded =
        SoundCache::new().load(&mut store, &AssetPath::new("Sound/Test/Position.wav")?)?;

    let _sdl_test = sdl_test_lock();
    let mut decoder = SoundDecoder::new()?;
    let sound = decoder.load(&encoded, SoundDecodeMode::Predecoded)?;
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let capacity = NonZeroU16::new(1).ok_or("voice capacity is zero")?;
    let mut backend = SoundBackend::new(&output, capacity, capacity)?;
    let voice = backend
        .play(&decoder, sound, 1.0, true, SoundVoicePriority::DEFAULT)?
        .voice();

    assert!(matches!(
        SoundSpatialPosition::new([f32::NAN, 0.0, 0.0]),
        Err(SoundBackendError::InvalidSpatialPosition { .. })
    ));
    backend.set_spatial_position(voice, Some(SoundSpatialPosition::new([1.0, 0.0, 0.0])?))?;
    let mut mixed = [0_u8; 4_096];
    backend.generate(&mut mixed)?;
    let (left, right) = stereo_energy(&mixed);
    assert!(left < right);

    backend.set_spatial_mix(
        voice,
        Some(SoundSpatialPosition::new([1.0, 0.0, 0.0])?),
        0.0,
    )?;
    let mut centered = [0_u8; 4_096];
    backend.generate(&mut centered)?;
    let (left, right) = stereo_energy(&centered);
    assert_eq!(left, right);
    assert!(matches!(
        backend.set_spatial_mix(
            voice,
            Some(SoundSpatialPosition::new([1.0, 0.0, 0.0])?),
            1.1,
        ),
        Err(SoundBackendError::InvalidSpatialPanLevel { .. })
    ));

    backend.set_spatial_position(voice, None)?;
    backend.stop(voice)?;
    Ok(())
}

/// Hard virtual-pool exhaustion steals the weakest current generation.
#[test]
fn reused_voice_rejects_stale_and_foreign_handles() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, &[0, 8_000, 0, -8_000])?;
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Sound\\Test\\Tone.wav",
        bytes: &wav,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let path = AssetPath::new("Sound/Test/Tone.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;

    let _sdl_test = sdl_test_lock();
    let mut decoder = SoundDecoder::new()?;
    let sound = decoder.load(&encoded, SoundDecodeMode::Predecoded)?;
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let capacity = NonZeroU16::new(1).ok_or("voice capacity is zero")?;
    let mut backend = SoundBackend::new(&output, capacity, capacity)?;
    let foreign_backend = SoundBackend::new(&output, capacity, capacity)?;

    assert!(matches!(
        backend.play(&decoder, sound, -0.1, false, SoundVoicePriority::DEFAULT),
        Err(SoundBackendError::InvalidGain { .. })
    ));
    let first = backend
        .play(&decoder, sound, 1.0, false, SoundVoicePriority::DEFAULT)?
        .voice();
    assert!(matches!(
        foreign_backend.state(first),
        Err(SoundBackendError::UnknownVoice)
    ));
    let replacement = backend.play(&decoder, sound, 1.0, false, SoundVoicePriority::new(100))?;
    assert_eq!(replacement.replaced(), Some(first));
    let second = replacement.voice();
    assert_ne!(second, first);
    assert!(matches!(
        backend.state(first),
        Err(SoundBackendError::UnknownVoice)
    ));
    backend.stop(second)?;
    Ok(())
}

/// Stock promotes a real default-priority one-shot, but not a looping voice.
#[test]
fn backend_promotes_only_real_default_priority_one_shots() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, &vec![8_000; 8_000])?;
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Sound\\Test\\Priority.wav",
        bytes: &wav,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let encoded =
        SoundCache::new().load(&mut store, &AssetPath::new("Sound/Test/Priority.wav")?)?;
    let _sdl_test = sdl_test_lock();
    let mut decoder = SoundDecoder::new()?;
    let sound = decoder.load(&encoded, SoundDecodeMode::Predecoded)?;
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let one = NonZeroU16::new(1).ok_or("software channel count is zero")?;
    let two = NonZeroU16::new(2).ok_or("virtual voice capacity is zero")?;
    let mut backend = SoundBackend::new(&output, one, two)?;

    let looping = backend
        .play(&decoder, sound, 1.0, true, SoundVoicePriority::DEFAULT)?
        .voice();
    assert_eq!(backend.effective_priority(looping)?, 128);
    backend.stop(looping)?;

    let one_shot = backend
        .play(&decoder, sound, 1.0, false, SoundVoicePriority::DEFAULT)?
        .voice();
    assert_eq!(backend.effective_priority(one_shot)?, 127);
    backend.stop(one_shot)?;
    Ok(())
}

/// Movie PCM streams through the existing mixer without whole-file admission.
#[test]
fn backend_streams_cinematic_pcm_on_a_dedicated_track() -> Result<(), Box<dyn Error>> {
    let _sdl_test = sdl_test_lock();
    let output = SoundOutput::open(SoundOutputTarget::Memory)?;
    let capacity = NonZeroU16::new(1).ok_or("voice capacity is zero")?;
    let mut backend = SoundBackend::new(&output, capacity, capacity)?;
    let samples = [0_i16, 12_000, 0, -12_000].repeat(22_050);

    backend.start_cinematic_audio(&samples, 1.0)?;
    assert_eq!(
        backend.cinematic_playback_time(),
        Some(std::time::Duration::ZERO)
    );
    let mut mixed = [0_u8; 4_096];
    let mixed_byte_count = backend.generate(&mut mixed)?;
    assert!(mixed_byte_count > 0);
    assert!(mixed.iter().any(|byte| *byte != 0));
    assert!(
        backend
            .cinematic_playback_time()
            .is_some_and(|time| !time.is_zero())
    );
    backend.queue_cinematic_audio(&samples[..4_096])?;
    backend.stop_cinematic_audio()?;
    assert!(matches!(
        backend.queue_cinematic_audio(&samples[..4_096]),
        Err(SoundBackendError::MissingCinematicTrack)
    ));
    Ok(())
}

/// Totals absolute signed-16 energy for an interleaved stereo output buffer.
fn stereo_energy(bytes: &[u8]) -> (u64, u64) {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .fold((0, 0), |(left, right), frame| {
            let left_sample = i16::from_le_bytes([frame[0], frame[1]]).unsigned_abs();
            let right_sample = i16::from_le_bytes([frame[2], frame[3]]).unsigned_abs();
            (
                left + u64::from(left_sample),
                right + u64::from(right_sample),
            )
        })
}
