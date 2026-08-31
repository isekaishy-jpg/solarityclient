//! External stock-compatibility tests for fixed-capacity SDL output voices.

use std::error::Error;
use std::num::NonZeroU16;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_media::{
    SoundBackend, SoundBackendError, SoundCache, SoundDecodeMode, SoundDecoder, SoundOutput,
    SoundOutputTarget, SoundSpatialPosition, SoundVoiceState,
};

use crate::support::{Fixture, FixtureFile, pcm_wav, sdl_test_lock};

/// A configured voice remains owned until it stops; exhaustion never steals it.
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
    let mut backend = SoundBackend::new(&output, capacity)?;
    assert_eq!(backend.voice_capacity(), 1);

    let voice = backend.play(&decoder, sound, 0.5, true)?;
    assert_eq!(backend.state(voice)?, SoundVoiceState::Playing);
    assert!(matches!(
        backend.play(&decoder, sound, 0.5, true),
        Err(SoundBackendError::VoiceCapacity)
    ));
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
    let mut backend = SoundBackend::new(&output, capacity)?;
    let voice = backend.play(&decoder, sound, 1.0, true)?;

    assert!(matches!(
        SoundSpatialPosition::new([f32::NAN, 0.0, 0.0]),
        Err(SoundBackendError::InvalidSpatialPosition { .. })
    ));
    backend.set_spatial_position(voice, Some(SoundSpatialPosition::new([1.0, 0.0, 0.0])?))?;
    let mut mixed = [0_u8; 4_096];
    backend.generate(&mut mixed)?;
    let (left, right) = stereo_energy(&mixed);
    assert!(left < right);

    backend.set_spatial_position(voice, None)?;
    backend.stop(voice)?;
    Ok(())
}

/// Reusing a stopped slot invalidates the previous generation only.
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
    let mut backend = SoundBackend::new(&output, capacity)?;
    let foreign_backend = SoundBackend::new(&output, capacity)?;

    assert!(matches!(
        backend.play(&decoder, sound, -0.1, false),
        Err(SoundBackendError::InvalidGain { .. })
    ));
    let first = backend.play(&decoder, sound, 1.0, false)?;
    assert!(matches!(
        foreign_backend.state(first),
        Err(SoundBackendError::UnknownVoice)
    ));
    backend.stop(first)?;
    let second = backend.play(&decoder, sound, 1.0, false)?;
    assert_ne!(second, first);
    assert!(matches!(
        backend.state(first),
        Err(SoundBackendError::UnknownVoice)
    ));
    backend.stop(second)?;
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
