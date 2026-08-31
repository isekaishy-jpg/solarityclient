//! External stock-compatibility tests for encoded-audio admission.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_media::{SoundCache, SoundDecodeMode, SoundDecoder};

use crate::support::{Fixture, FixtureFile, pcm_wav, sdl_test_lock};

/// Samples deduplicate while each stream owns a distinct decoder resource.
#[test]
fn wav_admission_preserves_format_and_mode_identity() -> Result<(), Box<dyn Error>> {
    let wav = pcm_wav(8_000, &[0, 1_000, -1_000, 0])?;
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Sound\\Test\\Pulse.wav",
        bytes: &wav,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let path = AssetPath::new("Sound/Test/Pulse.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;

    let _sdl_test = sdl_test_lock();
    let mut decoder = SoundDecoder::new()?;
    assert!(decoder.is_empty());
    let predecoded = decoder.load(&encoded, SoundDecodeMode::Predecoded)?;
    assert_eq!(
        decoder.load(&encoded, SoundDecodeMode::Predecoded)?,
        predecoded
    );
    let streaming = decoder.load(&encoded, SoundDecodeMode::Streaming)?;
    assert_ne!(streaming, predecoded);
    let second_streaming = decoder.load(&encoded, SoundDecodeMode::Streaming)?;
    assert_ne!(second_streaming, streaming);
    assert_eq!(decoder.len(), 3);

    let info = decoder
        .info(predecoded)
        .ok_or("decoded resource is absent")?;
    assert_eq!(info.path(), &path);
    assert_eq!(info.mode(), SoundDecodeMode::Predecoded);
    assert_eq!(info.sample_rate_hz(), 8_000);
    assert_eq!(info.channel_count(), 1);
    assert_eq!(info.duration_frames(), Some(4));
    assert!(decoder.release_streaming(streaming));
    assert!(decoder.release_streaming(second_streaming));
    assert!(!decoder.release_streaming(predecoded));
    assert_eq!(decoder.len(), 1);
    Ok(())
}

/// Unsupported bytes fail admission and do not poison the registry.
#[test]
fn invalid_encoded_sound_has_no_decoder_fallback() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Sound\\Test\\Broken.wav",
        bytes: b"not an encoded stock sound",
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let path = AssetPath::new("Sound/Test/Broken.wav")?;
    let encoded = SoundCache::new().load(&mut store, &path)?;

    let _sdl_test = sdl_test_lock();
    let mut decoder = SoundDecoder::new()?;
    assert!(decoder.load(&encoded, SoundDecodeMode::Predecoded).is_err());
    assert!(decoder.is_empty());
    Ok(())
}
