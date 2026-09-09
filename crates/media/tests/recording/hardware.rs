//! Actual encoder and decoder exercise, explicitly opted into on supported hardware.

use std::error::Error;
use std::time::{Duration, Instant};

use ffmpeg_next as ffmpeg;

use super::Directory;
use crate::recording::audio::{AUDIO_BLOCK_FRAMES, AudioBlock};
use crate::recording::encoder::RecordingEncoder;
use crate::recording::files::{RecordingFiles, RecordingMode};

/// Fast-forwarding actual timestamps proves the native muxer wraps its owned file ring.
#[test]
#[ignore = "requires hardware H.264; encodes eight minutes to exercise real segment wrapping"]
fn automated_hardware_recording_wraps_eight_playable_segments() -> Result<(), Box<dyn Error>> {
    let directory = Directory::new()?;
    let files = RecordingFiles::create(&directory.0, RecordingMode::Automated)?;
    std::fs::write(&files.path, b"unrelated literal-pattern filename")?;
    let started = Instant::now();
    let mut encoder = RecordingEncoder::create(&files, (320, 180), 48_000)?;
    let pixels = [30, 60, 120, 255].repeat(320 * 180);
    for second in 0..481 {
        encoder.image(&pixels, Duration::from_secs(second))?;
        encoder.video_until((second + 1) * 30)?;
        encoder.silence_until((second + 1) * 48_000)?;
        files.maintain(Some(encoder.active_segment))?;
    }
    encoder.finish(Duration::from_secs(481))?;
    drop(encoder);
    let mut bytes = 0;
    for index in 0..8 {
        let path = directory
            .0
            .join("Automated")
            .join(format!("check-{index:02}.mp4"));
        bytes += std::fs::metadata(&path)?.len();
        let input = ffmpeg::format::input(&path)?;
        assert!(input.streams().best(ffmpeg::media::Type::Video).is_some());
        assert!(input.streams().best(ffmpeg::media::Type::Audio).is_some());
        assert!(
            input.duration() > 0 && input.duration() < 62_000_000,
            "segment {index}: {}",
            input.duration()
        );
    }
    assert!(bytes <= 256 * 1024 * 1024);
    assert!(!directory.0.join("Automated/check-08.mp4").exists());
    assert_eq!(
        std::fs::read(&files.path)?,
        b"unrelated literal-pattern filename"
    );
    eprintln!(
        "481 seconds rotated across eight clips in {:?}; {bytes} bytes retained",
        started.elapsed()
    );
    Ok(())
}

/// Decoding proves that BGRA conversion, AAC audio, holds, and timestamps survive muxing.
#[test]
#[ignore = "requires a working Media Foundation hardware H.264 encoder"]
fn hardware_mp4_roundtrip_preserves_picture_audio_and_timing() -> Result<(), Box<dyn Error>> {
    let directory = Directory::new()?;
    let files = RecordingFiles::create(&directory.0, RecordingMode::Manual)?;
    let started = Instant::now();
    let mut encoder = RecordingEncoder::create(&files, (320, 180), 48_000)?;
    let red = [0, 0, 255, 255].repeat(320 * 180);
    let blue = [255, 0, 0, 255].repeat(320 * 180);
    encoder.image(&red, Duration::ZERO)?;
    for first in (0..96_000).step_by(AUDIO_BLOCK_FRAMES) {
        let frames = AUDIO_BLOCK_FRAMES.min(96_000 - first);
        let mut block = AudioBlock {
            first_frame: first as u64,
            frames,
            samples: [0.0; AUDIO_BLOCK_FRAMES * 2],
        };
        for (offset, pair) in block.samples[..frames * 2]
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
        {
            let value =
                (((first + offset) as f32 * 440.0 * std::f32::consts::TAU) / 48_000.0).sin() * 0.25;
            pair.copy_from_slice(&[value, -value]);
        }
        encoder.audio(block)?;
    }
    encoder.image(&blue, Duration::from_secs(1))?;
    encoder.finish(Duration::from_secs(2))?;
    assert_eq!(encoder.video_frames, 60);
    drop(encoder);
    eprintln!(
        "two-second H.264/AAC clip encoded in {:?}",
        started.elapsed()
    );
    let mut input = ffmpeg::format::input(&files.path)?;
    let video = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or("missing video")?;
    assert_eq!(video.avg_frame_rate(), ffmpeg::Rational(30, 1));
    let video_index = video.index();
    let mut video_decoder = ffmpeg::codec::context::Context::from_parameters(video.parameters())?
        .decoder()
        .video()?;
    let audio = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or("missing audio")?;
    let audio_index = audio.index();
    let mut audio_decoder = ffmpeg::codec::context::Context::from_parameters(audio.parameters())?
        .decoder()
        .audio()?;
    let mut video_frames = Vec::new();
    let mut audio_samples = 0;
    let mut peak = 0.0_f32;
    for (stream, packet) in input.packets() {
        if stream.index() == video_index {
            video_decoder.send_packet(&packet)?;
            drain_video(&mut video_decoder, &mut video_frames)?;
        } else if stream.index() == audio_index {
            audio_decoder.send_packet(&packet)?;
            drain_audio(&mut audio_decoder, &mut audio_samples, &mut peak)?;
        }
    }
    video_decoder.send_eof()?;
    drain_video(&mut video_decoder, &mut video_frames)?;
    audio_decoder.send_eof()?;
    drain_audio(&mut audio_decoder, &mut audio_samples, &mut peak)?;
    assert_eq!(video_frames.len(), 60);
    assert!(
        video_frames[0][0] > 230 && video_frames[0][2] < 20,
        "{:?}",
        video_frames[0]
    );
    assert!(
        video_frames[59][2] > 230 && video_frames[59][0] < 20,
        "{:?}",
        video_frames[59]
    );
    assert!(
        (96_000..=98_048).contains(&audio_samples),
        "{audio_samples}"
    );
    assert!(peak > 0.2 && peak < 0.35, "{peak}");
    assert!(
        (input.duration() - 2_000_000).abs() < 60_000,
        "{}",
        input.duration()
    );
    Ok(())
}

/// Collect one decoded pixel from each frame while checking terminal decoder errors.
fn drain_video(
    decoder: &mut ffmpeg::decoder::Video,
    pixels: &mut Vec<[u8; 3]>,
) -> Result<(), Box<dyn Error>> {
    let mut decoded = ffmpeg::frame::Video::empty();
    loop {
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                let mut scaler = ffmpeg::software::scaling::Context::get(
                    decoded.format(),
                    decoded.width(),
                    decoded.height(),
                    ffmpeg::format::Pixel::RGB24,
                    decoded.width(),
                    decoded.height(),
                    ffmpeg::software::scaling::Flags::BILINEAR,
                )?;
                let mut rgb = ffmpeg::frame::Video::empty();
                scaler.run(&decoded, &mut rgb)?;
                pixels.push(rgb.data(0)[..3].try_into()?);
            }
            Err(ffmpeg::Error::Eof) => return Ok(()),
            Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => {
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// Audio decoding verifies actual samples rather than merely an advertised stream.
fn drain_audio(
    decoder: &mut ffmpeg::decoder::Audio,
    samples: &mut usize,
    peak: &mut f32,
) -> Result<(), Box<dyn Error>> {
    let mut decoded = ffmpeg::frame::Audio::empty();
    loop {
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                *samples += decoded.samples();
                for value in decoded.plane::<f32>(0) {
                    *peak = peak.max(value.abs());
                }
            }
            Err(ffmpeg::Error::Eof) => return Ok(()),
            Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => {
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        }
    }
}
