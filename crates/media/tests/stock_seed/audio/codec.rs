//! External stock-compatibility tests for encoded-audio admission.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_media::{SoundCache, SoundDecodeMode, SoundDecoder};

use crate::support::{Fixture, FixtureFile, sdl_test_lock};

/// One WAV path deduplicates within each explicit residency strategy.
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
    assert_eq!(decoder.len(), 2);

    let info = decoder
        .info(predecoded)
        .ok_or("decoded resource is absent")?;
    assert_eq!(info.path(), &path);
    assert_eq!(info.mode(), SoundDecodeMode::Predecoded);
    assert_eq!(info.sample_rate_hz(), 8_000);
    assert_eq!(info.channel_count(), 1);
    assert_eq!(info.duration_frames(), Some(4));
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

/// Builds a mono signed-16 PCM RIFF/WAVE fixture.
fn pcm_wav(sample_rate_hz: u32, samples: &[i16]) -> Result<Vec<u8>, Box<dyn Error>> {
    const FORMAT_CHUNK_SIZE: u32 = 16;
    const PCM_FORMAT: u16 = 1;
    const CHANNEL_COUNT: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;

    let data_byte_count = u32::try_from(samples.len())?
        .checked_mul(size_of::<i16>() as u32)
        .ok_or("WAV fixture byte count overflows")?;
    let mut bytes = Vec::with_capacity(44 + data_byte_count as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_byte_count).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&FORMAT_CHUNK_SIZE.to_le_bytes());
    bytes.extend_from_slice(&PCM_FORMAT.to_le_bytes());
    bytes.extend_from_slice(&CHANNEL_COUNT.to_le_bytes());
    bytes.extend_from_slice(&sample_rate_hz.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate_hz * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_byte_count.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(bytes)
}
