//! Decodes a bounded number of frames from one installed stock AVI.

use std::error::Error;
use std::path::PathBuf;

use solarity_media::CinematicDecoder;

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: validate_cinematic <movie.avi> [frame-count]")?;
    let count = std::env::args()
        .nth(2)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(3);
    let mut decoder = CinematicDecoder::open(&path)?;
    let mut audio_samples = 0_usize;
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
