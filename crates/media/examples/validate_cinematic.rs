//! Decodes a bounded number of frames or one complete installed stock AVI.

use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use solarity_media::CinematicDecoder;

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: validate_cinematic <movie.avi> [frame-count|all]")?;
    let requested = std::env::args().nth(2).unwrap_or_else(|| "3".to_owned());
    let mut decoder = CinematicDecoder::open(&path)?;
    let mut audio_samples = 0_usize;
    if requested == "all" {
        let mut frame_count = 0_u64;
        let mut final_pts = Duration::ZERO;
        while let Some(frame) = decoder.next_video_frame()? {
            if frame.presentation_time() < final_pts {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "cinematic video clock moved backward",
                )
                .into());
            }
            final_pts = frame.presentation_time();
            frame_count = frame_count.checked_add(1).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "cinematic frame count overflowed",
                )
            })?;
            audio_samples += decoder
                .take_audio_frames()
                .iter()
                .map(|frame| frame.samples().len())
                .sum::<usize>();
        }
        audio_samples += decoder
            .take_audio_frames()
            .iter()
            .map(|frame| frame.samples().len())
            .sum::<usize>();
        println!(
            "decoded complete movie: frames={frame_count} final_pts_ms={} audio_samples={audio_samples}",
            final_pts.as_millis()
        );
        return Ok(());
    }
    let count = requested.parse::<usize>()?;
    for index in 0..count {
        let frame = decoder
            .next_video_frame()?
            .ok_or("movie ended before the requested validation frame count")?;
        audio_samples += decoder
            .take_audio_frames()
            .iter()
            .map(|frame| frame.samples().len())
            .sum::<usize>();
        println!(
            "frame={index} extent={}x{} pts_ms={} bytes={} audio_samples={audio_samples}",
            frame.width(),
            frame.height(),
            frame.presentation_time().as_millis(),
            frame.rgba8().len()
        );
    }
    Ok(())
}
