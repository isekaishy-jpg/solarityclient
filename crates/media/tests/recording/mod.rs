//! Queue pressure, retention, and opt-in actual hardware codec verification.

mod hardware;

use std::error::Error;
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::audio::{AUDIO_BLOCK_FRAMES, RecordingAudio};
use super::files::{RecordingFiles, RecordingMode};

/// Isolated roots have deterministic ownership and are removed after each test.
struct Directory(PathBuf);

impl Directory {
    /// Process identity and a monotonic counter isolate concurrently running tests.
    fn new() -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "solarity-recording-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _removed = fs::remove_dir_all(&self.0);
    }
}

/// SDL's final mix callback reports float counts and detaches before userdata drops.
#[test]
fn memory_mixer_tap_captures_stereo_frames_and_detaches() -> Result<(), Box<dyn Error>> {
    let _sdl = sdl3::init()?;
    let output = crate::SoundOutput::open(crate::SoundOutputTarget::Memory)?;
    let audio = std::sync::Arc::new(RecordingAudio::new(
        Instant::now(),
        output.info().sample_rate_hz(),
    ));
    output.set_recording_audio(Some(std::sync::Arc::clone(&audio)))?;
    let mut bytes = [0_u8; 4096];
    output.generate(&mut bytes)?;
    let mut captured = 0;
    while let Some(block) = audio.pop()? {
        captured += block.frames;
    }
    assert_eq!(captured, 1024);
    assert!(!audio.changed_format());
    output.set_recording_audio(None)?;
    output.generate(&mut bytes)?;
    assert!(audio.pop()?.is_none());
    Ok(())
}

/// Congestion loses capture samples without blocking or shifting their source clock.
#[test]
fn audio_pressure_preserves_source_timestamps_and_bounds_storage() -> Result<(), Box<dyn Error>> {
    let audio = RecordingAudio::new(Instant::now(), 48_000);
    let samples = vec![0.25; AUDIO_BLOCK_FRAMES * 2];
    for index in 0..65 {
        audio.capture(index * AUDIO_BLOCK_FRAMES as u64, &samples, 48_000, 2);
    }
    assert_eq!(audio.dropped_frames(), AUDIO_BLOCK_FRAMES as u64);
    for index in 0..64 {
        let block = audio.pop()?.ok_or("missing captured audio")?;
        assert_eq!(block.first_frame, index * AUDIO_BLOCK_FRAMES as u64);
        assert_eq!(block.samples[0], 0.25);
    }
    assert!(audio.pop()?.is_none());
    audio.capture(70_000, &[0.5, -0.5], 48_000, 2);
    assert_eq!(
        audio.pop()?.ok_or("missing resumed audio")?.first_frame,
        70_000
    );
    audio.capture(70_001, &[0.5, -0.5], 44_100, 2);
    assert!(audio.changed_format());
    Ok(())
}

/// Only owned automated clips are removed; manual files and the active clip survive.
#[test]
fn retention_enforces_budget_and_exclusive_ownership() -> Result<(), Box<dyn Error>> {
    let root = Directory::new()?;
    let manual = RecordingFiles::create(&root.0, RecordingMode::Manual)?;
    fs::write(&manual.path, b"manual evidence")?;
    let files = RecordingFiles::create(&root.0, RecordingMode::Automated)?;
    assert!(RecordingFiles::create(&root.0, RecordingMode::Automated).is_err());
    let automatic = root.0.join("Automated");
    assert_eq!(files.first_segment, 0);
    fs::write(automatic.join("check-00.mp4"), b"previous automated check")?;
    drop(files);
    let files = RecordingFiles::create(&root.0, RecordingMode::Automated)?;
    assert_eq!(files.first_segment, 1);
    fs::write(automatic.join("unrelated.mp4"), b"unrelated")?;
    for index in 0..8 {
        File::create(automatic.join(format!("check-{index:02}.mp4")))?.set_len(40 * 1024 * 1024)?;
    }
    files.maintain(Some(0))?;
    let bytes: u64 = (0..8)
        .filter_map(|index| fs::metadata(automatic.join(format!("check-{index:02}.mp4"))).ok())
        .map(|m| m.len())
        .sum();
    assert!(bytes <= 248 * 1024 * 1024);
    assert!(automatic.join("check-00.mp4").exists());
    assert_eq!(fs::read(&manual.path)?, b"manual evidence");
    assert_eq!(fs::read(automatic.join("unrelated.mp4"))?, b"unrelated");
    drop(files);
    assert!(RecordingFiles::create(&root.0, RecordingMode::Automated).is_ok());
    Ok(())
}
